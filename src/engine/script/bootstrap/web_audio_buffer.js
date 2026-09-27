    const audioBufferData = new WeakMap();
    const audioBufferMetadata = new WeakMap();
    const audioBufferInfo = buffer => {
        const info = audioBufferMetadata.get(buffer);
        if (!info) throw new TypeError('Illegal AudioBuffer invocation');
        return info;
    };

    class AudioBuffer {
        constructor(options) {
            if (options === null || typeof options !== 'object')
                throw new TypeError('AudioBuffer requires an options dictionary');
            const channels = validChannels(options.numberOfChannels === undefined ? 1 :
                options.numberOfChannels);
            const length = validLength(options.length);
            const rate = validSampleRate(options.sampleRate);
            if (length * channels * 4 > MAX_AUDIO_BUFFER_BYTES)
                throw new DOMException('Audio buffer exceeds the implementation limit', 'NotSupportedError');
            audioBufferData.set(this,
                Array.from({ length: channels }, () => new Float32Array(length)));
            audioBufferMetadata.set(this,
                { numberOfChannels: channels, length, sampleRate: rate,
                    duration: length / rate });
        }
        getChannelData(channel) {
            channel = Number(channel) >>> 0;
            if (channel >= this.numberOfChannels)
                throw new DOMException('Channel index is out of range', 'IndexSizeError');
            return audioBufferData.get(this)[channel];
        }
        copyFromChannel(destination, channel, startInChannel = 0) {
            if (!(destination instanceof Float32Array))
                throw new TypeError('Destination must be a Float32Array');
            const source = this.getChannelData(channel);
            const start = Number(startInChannel) >>> 0;
            if (start >= this.length) return;
            destination.set(source.subarray(start, start + destination.length));
        }
        copyToChannel(source, channel, startInChannel = 0) {
            if (!(source instanceof Float32Array))
                throw new TypeError('Source must be a Float32Array');
            const destination = this.getChannelData(channel);
            const start = Number(startInChannel) >>> 0;
            if (start >= this.length) return;
            destination.set(source.subarray(0, this.length - start), start);
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
