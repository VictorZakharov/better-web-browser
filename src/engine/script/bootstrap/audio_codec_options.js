    const audioCodecStates = new WeakMap();
    const audioCodecTrustedEvent = globalThis.__markTrustedEvent;
    const audioCodecFinalizer = new FinalizationRegistry(lifetime => {
        if (lifetime.id !== null) host('audioCodecClose', lifetime.id);
    });
    const opusEncoderOptions = value => {
        const input = frameDictionary(value);
        const application = frameEnum(input.application, ['audio', 'voip', 'lowdelay'], 'audio');
        const rawComplexity = input.complexity;
        const complexity = rawComplexity === undefined ? 9 : frameUint(rawComplexity);
        const format = frameEnum(input.format, ['opus', 'ogg'], 'opus');
        const rawDuration = input.frameDuration;
        const frameDuration = rawDuration === undefined ? 20000 : frameTime(rawDuration, true);
        const rawLoss = input.packetlossperc;
        const packetlossperc = rawLoss === undefined ? 0 : frameUint(rawLoss);
        const signal = frameEnum(input.signal, ['auto', 'music', 'voice'], 'auto');
        const usedtx = Boolean(input.usedtx);
        const useinbandfec = Boolean(input.useinbandfec);
        if (frameDuration < 2500 || frameDuration > 120000 || frameDuration % 2500 || complexity > 10 || packetlossperc > 100)
            throw new TypeError('Invalid Opus encoder options');
        return {application, complexity, format, frameDuration, packetlossperc, signal, usedtx, useinbandfec};
    };
    const audioCodecConfig = (value, encode) => {
        const input = frameDictionary(value);
        const config = {};
        if (encode) {
            const bitrate = input.bitrate;
            if (bitrate !== undefined) config.bitrate = frameTime(bitrate, true);
            config.bitrateMode = frameEnum(input.bitrateMode, ['constant', 'variable'], 'variable');
        }
        config.codec = frameString(frameRequired(input.codec, 'codec'));
        if (!encode) {
            const description = input.description;
            if (description !== undefined) {
                const bytes = frameBuffer(description);
                if (bytes.length > 256) throw frameError('Codec description exceeds its 256 byte budget', 'NotSupportedError');
                config.description = new Uint8Array(bytes);
            }
        }
        config.numberOfChannels = frameUint(frameRequired(input.numberOfChannels, 'numberOfChannels'));
        if (encode) {
            const opus = input.opus;
            if (opus !== undefined) config.opus = opusEncoderOptions(opus);
        }
        config.sampleRate = frameUint(frameRequired(input.sampleRate, 'sampleRate'));
        if (!config.codec.replace(/^[\t\n\f\r ]+|[\t\n\f\r ]+$/g, '') || !config.numberOfChannels || !config.sampleRate)
            throw new TypeError('Audio codec name, sample rate and channel count must be nonempty');
        return config;
    };
    const audioCodecNativeConfig = config => JSON.stringify({...config,
        ...(config.description === undefined ? {} : {description: Array.from(config.description)})});
    const audioCodecSupported = (config, encode) => Boolean(host('audioCodecSupported', encode, audioCodecNativeConfig(config)));
    const audioCodecState = (value, encode) => {
        const state = audioCodecStates.get(value);
        if (!state || (encode !== undefined && state.encode !== encode))
            throw new TypeError('Illegal audio codec receiver');
        return state;
    };
    const configuredAudioCodec = (value, encode) => {
        const state = audioCodecState(value, encode);
        if (state.status !== 'configured') throw frameError('Audio codec is not configured');
        return state;
    };
    const audioCodecSupport = (value, encode) => {
        try {
            const config = audioCodecConfig(value, encode);
            return new Promise((resolve, reject) => setTimeout(() => {
                try { resolve({supported: audioCodecSupported(config, encode), config}); }
                catch (error) { reject(error); }
            }, 0));
        } catch (error) { return Promise.reject(error); }
    };
