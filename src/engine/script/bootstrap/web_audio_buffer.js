    const audioBufferData = new WeakMap();
    const audioBufferMetadata = new WeakMap();
    const audioBufferInfo = buffer => {
        const info = audioBufferMetadata.get(buffer);
        if (!info) throw new TypeError('Illegal AudioBuffer invocation');
        return info;
    };
    const audioBufferChannel = (buffer, channel, info) => {
        if (channel >= info.numberOfChannels)
            throw new AudioDOMException('Channel index is out of range', 'IndexSizeError');
        return audioBufferData.get(buffer)[channel];
    };
    const audioTypedArrayPrototype = Object.getPrototypeOf(Float32Array.prototype);
    const audioTypedArrayTag = Object.getOwnPropertyDescriptor(audioTypedArrayPrototype,
        Symbol.toStringTag).get;
    const audioTypedArrayBuffer = Object.getOwnPropertyDescriptor(audioTypedArrayPrototype, 'buffer').get;
    const audioTypedArrayLength = Object.getOwnPropertyDescriptor(audioTypedArrayPrototype, 'length').get;
    const audioTypedArrayOffset = Object.getOwnPropertyDescriptor(audioTypedArrayPrototype, 'byteOffset').get;
    const audioTypedArraySet = Float32Array.prototype.set;
    const AudioChannelFloat32Array = Float32Array;
    const audioArrayBufferLength = Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, 'byteLength').get;
    const audioArrayBufferResizable = Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, 'resizable')?.get;
    const audioFloat32Argument = array => {
        if (audioTypedArrayTag.call(array) !== 'Float32Array')
            throw new TypeError('Audio channel copy requires a Float32Array');
        const buffer = audioTypedArrayBuffer.call(array);
        // These Web IDL arguments have neither AllowShared nor AllowResizable.
        // A detached fixed-length view is still a valid, empty IDL view.
        audioArrayBufferLength.call(buffer);
        if (audioArrayBufferResizable?.call(buffer))
            throw new TypeError('Audio channel copy requires fixed-length storage');
        return array;
    };
    const copyAudioChannelSamples = (source, destination, sourceOffset, destinationOffset) => {
        const count = Math.min(audioTypedArrayLength.call(source) - sourceOffset,
            audioTypedArrayLength.call(destination) - destinationOffset);
        if (count <= 0) return;
        // Native copy never invokes author-overridden length/set/subarray or
        // TypedArray species. Intrinsic set also preserves overlapping views.
        const view = new AudioChannelFloat32Array(audioTypedArrayBuffer.call(source),
            audioTypedArrayOffset.call(source) + sourceOffset * 4, count);
        audioTypedArraySet.call(destination, view, destinationOffset);
    };
    const audioBufferOptions = input => {
        if (input != null && typeof input !== 'object' && typeof input !== 'function')
            throw new TypeError('AudioBuffer requires an options dictionary');
        const options = input ?? {};
        const length = options.length;
        if (length === undefined) throw new TypeError('AudioBuffer requires length');
        const frames = audioChannelUnsigned(length);
        const channels = audioOption(options, 'numberOfChannels', 1, audioChannelUnsigned);
        const sampleRate = options.sampleRate;
        if (sampleRate === undefined) throw new TypeError('AudioBuffer requires sampleRate');
        const rate = finiteFloat(sampleRate, 'sampleRate');
        if (!frames || channels < 1 || channels > 32 || rate < 8000 || rate > 192000)
            throw new AudioDOMException('Unsupported AudioBuffer length, channel count, or sample rate',
                'NotSupportedError');
        if (frames * channels * 4 > MAX_AUDIO_BUFFER_BYTES)
            throw new AudioDOMException('Audio buffer exceeds the implementation limit', 'NotSupportedError');
        return { numberOfChannels: channels, length: frames, sampleRate: rate, duration: frames / rate };
    };

    class AudioBuffer {
        constructor(options) {
            if (!arguments.length) throw new TypeError('AudioBuffer requires options');
            const info = audioBufferOptions(options);
            audioBufferData.set(this,
                silence(info.numberOfChannels, info.length));
            audioBufferMetadata.set(this, info);
        }
        getChannelData(channel) {
            const info = audioBufferInfo(this);
            if (!arguments.length) throw new TypeError('getChannelData requires a channel index');
            return audioBufferChannel(this, audioChannelUnsigned(channel), info);
        }
        copyFromChannel(destination, channel, startInChannel = 0) {
            const info = audioBufferInfo(this);
            if (arguments.length < 2) throw new TypeError('copyFromChannel requires an array and channel');
            audioFloat32Argument(destination);
            channel = audioChannelUnsigned(channel);
            const start = audioChannelUnsigned(startInChannel);
            const source = audioBufferChannel(this, channel, info);
            if (start < info.length) copyAudioChannelSamples(source, destination, start, 0);
        }
        copyToChannel(source, channel, startInChannel = 0) {
            const info = audioBufferInfo(this);
            if (arguments.length < 2) throw new TypeError('copyToChannel requires an array and channel');
            audioFloat32Argument(source);
            channel = audioChannelUnsigned(channel);
            const start = audioChannelUnsigned(startInChannel);
            const destination = audioBufferChannel(this, channel, info);
            if (start < info.length) copyAudioChannelSamples(source, destination, 0, start);
        }
    }
    // Web IDL readonly attributes are enumerable prototype accessors, not mutable
    // data slots on each AudioBuffer instance.
    for (const name of ['numberOfChannels', 'length', 'sampleRate', 'duration']) {
        Object.defineProperty(AudioBuffer.prototype, name, {
            configurable: true, enumerable: true,
            get() { return audioBufferInfo(this)[name]; }
        });
    }
    for (const name of ['getChannelData', 'copyFromChannel', 'copyToChannel'])
        Object.defineProperty(AudioBuffer.prototype, name, {
            ...Object.getOwnPropertyDescriptor(AudioBuffer.prototype, name), enumerable: true
        });
