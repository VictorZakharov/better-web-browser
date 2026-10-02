    const audioCodecAbort = state => {
        state.generation++;
        if (state.timer !== null) clearTimeout(state.timer);
        state.timer = null;
        if (state.id !== null) host('audioCodecClose', state.id);
        state.id = null;
        state.lifetime.id = null;
        state.active = null;
        state.queue = [];
        state.bytes = 0;
        const error = frameError('Audio codec work was aborted', 'AbortError');
        for (const request of state.flushes) request.reject(error);
        state.flushes.clear();
        if (state.size) { state.size = 0; audioCodecDequeue(state); }
        state.config = null;
        state.keyRequired = true;
        state.metadataPending = true;
    };
    const audioCodecDequeue = state => {
        if (state.dequeueScheduled) return;
        state.dequeueScheduled = true;
        setTimeout(() => {
            try {
                EventTarget.prototype.dispatchEvent.call(state.owner, audioCodecTrustedEvent(new Event('dequeue')));
            } finally { state.dequeueScheduled = false; }
        }, 0);
    };
    const audioCodecFail = (state, error, name = 'EncodingError') => {
        audioCodecAbort(state);
        state.status = 'closed';
        const exception = error instanceof DOMException ? error : frameError(String(error?.message ?? error), name);
        try { state.error.call(undefined, exception); }
        catch (callbackError) { setTimeout(() => { throw callbackError; }, 0); }
    };
    const audioCodecFatal = (state, error, name = 'EncodingError') => {
        const exception = error instanceof DOMException ? error : frameError(String(error?.message ?? error), name);
        for (const request of state.flushes) request.reject(exception);
        state.flushes.clear();
        audioCodecFail(state, exception);
    };
    const audioCodecOutputs = (state, outputs, generation) => {
        for (const result of outputs) {
            if (state.generation !== generation || state.status === 'closed') return;
            let value, metadata;
            if (state.encode) {
                value = new EncodedAudioChunk({type:'key', timestamp:result.timestamp,
                    duration:result.duration, data:result.bytes});
                metadata = {};
                if (state.metadataPending || result.description !== null) {
                    metadata.decoderConfig = {codec:state.config.codec, sampleRate:result.sampleRate,
                        numberOfChannels:result.numberOfChannels};
                    if (result.description !== null) metadata.decoderConfig.description = result.description.buffer.slice(0);
                    state.metadataPending = false;
                }
            } else {
                value = new AudioData({format:result.format, sampleRate:result.sampleRate,
                    numberOfFrames:result.frames, numberOfChannels:result.numberOfChannels,
                    timestamp:result.timestamp, data:result.bytes});
            }
            // Author callback exceptions are reported, never interpreted as a
            // codec failure, and never prevent flush from draining other outputs.
            try { if (state.encode) state.output.call(undefined, value, metadata);
                else state.output.call(undefined, value); }
            catch (error) { setTimeout(() => { throw error; }, 0); }
        }
    };
    const audioCodecPoll = state => {
        const generation = state.generation;
        state.timer = setTimeout(() => {
            state.timer = null;
            if (state.generation !== generation || state.status === 'closed') return;
            try {
                const result = host('audioCodecPoll', state.id);
                if (result.status === 'pending') { audioCodecPoll(state); return; }
                if (result.status !== 'ready') { audioCodecFatal(state, result.message); return; }
                const command = state.active;
                state.active = null;
                audioCodecOutputs(state, result.outputs, generation);
                if (state.generation !== generation) return;
                if (command.kind === 'configure' && command.phase === 'drain') {
                    state.active = command;
                    startAudioCodecConfiguration(state, command);
                    audioCodecPoll(state);
                    return;
                }
                if (command.kind === 'flush') {
                    state.flushes.delete(command);
                    command.resolve();
                }
                audioCodecRun(state);
            } catch (error) { if (state.generation === generation) audioCodecFatal(state, error); }
        }, 1);
    };
    const startAudioCodecConfiguration = (state, command) => {
        if (state.id !== null) host('audioCodecClose', state.id);
        state.config = command.config;
        state.metadataPending = true;
        state.id = host('audioCodecStart', state.encode, audioCodecNativeConfig(command.config));
        state.lifetime.id = state.id;
        command.phase = 'start';
    };
    const audioCodecRun = state => {
        if (state.active || state.timer !== null || !state.queue.length || state.status === 'closed') return;
        const command = state.queue.shift();
        state.active = command;
        try {
            if (command.kind === 'configure') {
                if (!audioCodecSupported(command.config, state.encode)) {
                    audioCodecFatal(state, 'Unsupported audio codec configuration', 'NotSupportedError');
                    return;
                }
                if (state.encode && state.id !== null) {
                    // Drain accepted PCM under its old configuration before a
                    // native implementation replacement can drop a partial frame.
                    command.phase = 'drain';
                    host('audioCodecFlush', state.id);
                } else startAudioCodecConfiguration(state, command);
            } else if (command.kind === 'flush') {
                host('audioCodecFlush', state.id);
            } else {
                state.size--;
                state.bytes -= command.bytes.length;
                audioCodecDequeue(state);
                if (state.encode && (command.rate !== state.config.sampleRate || command.channels !== state.config.numberOfChannels)) {
                    audioCodecFatal(state, 'AudioData rate or channels differ from the encoder configuration');
                    return;
                }
                host('audioCodecInput', state.id, command.bytes, command.timestamp);
                command.bytes = null;
            }
            audioCodecPoll(state);
        } catch (error) { audioCodecFatal(state, error); }
    };
    const enqueueAudioCodec = (state, command) => {
        const bytes = command.bytes?.length ?? 0;
        if (state.queue.length >= 64 || state.bytes + bytes > 8 * 1024 * 1024)
            throw frameError('Audio codec queue exceeds 64 commands or 8 MiB', 'QuotaExceededError');
        state.queue.push(command);
        if (command.kind === 'input') { state.size++; state.bytes += bytes; }
        // Acceptance is synchronous; codec tasks and callbacks are not. Do not
        // reduce the author-visible queue size in the enqueueing script turn.
        if (state.timer === null && !state.active) {
            state.timer = setTimeout(() => { state.timer = null; audioCodecRun(state); }, 0);
        }
    };
