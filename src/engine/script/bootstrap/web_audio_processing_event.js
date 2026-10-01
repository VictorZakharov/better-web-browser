    const audioProcessingEventState = new WeakMap();
    const audioEventBuffer = (options, name) => {
        const buffer = options[name];
        if (!audioBufferMetadata.has(buffer))
            throw new TypeError(name + ' must be an AudioBuffer');
        return buffer;
    };
    class AudioProcessingEvent extends AudioEvent {
        constructor(type, options) {
            if (arguments.length < 2) throw new TypeError('AudioProcessingEvent requires an event dictionary');
            type = `${type}`;
            if (options != null && typeof options !== 'object' && typeof options !== 'function')
                throw new TypeError('AudioProcessingEvent options must be a dictionary');
            options ??= {};
            // EventInit is converted first; derived members follow lexical order.
            const init = { bubbles: !!options.bubbles, cancelable: !!options.cancelable,
                composed: !!options.composed };
            const inputBuffer = audioEventBuffer(options, 'inputBuffer');
            const outputBuffer = audioEventBuffer(options, 'outputBuffer');
            const suppliedTime = options.playbackTime;
            if (suppliedTime === undefined) throw new TypeError('playbackTime is required');
            const playbackTime = finite(suppliedTime, 'playbackTime');
            super(type, init);
            audioProcessingEventState.set(this, { inputBuffer, outputBuffer, playbackTime });
        }
    }
    for (const name of ['inputBuffer', 'outputBuffer', 'playbackTime'])
        Object.defineProperty(AudioProcessingEvent.prototype, name, {
            enumerable: true, configurable: true,
            get() {
                const state = audioProcessingEventState.get(this);
                if (!state) throw new TypeError('Illegal AudioProcessingEvent receiver');
                return state[name];
            }
        });

    const processingAudioBuffer = (channels, length, rate) => {
        // ScriptProcessor permits zero input or output channels, but not both.
        // Its event buffer must reflect that zero-channel side. Public
        // AudioBuffer construction still requires at least one channel.
        const buffer = Object.create(AudioBuffer.prototype);
        audioBufferData.set(buffer, silence(channels, length));
        audioBufferMetadata.set(buffer, { numberOfChannels: channels, length,
            sampleRate: rate, duration: length / rate });
        return buffer;
    };
