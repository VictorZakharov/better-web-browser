    const audioCloneBindings = {
        has: value => audioDataStates.has(value),
        hasChunk: value => encodedAudioStates.has(value) || encodedVideoStates.has(value),
        closed: value => audioDataStates.get(value)?.bytes === null,
        detach: closeAudioData,
        snapshot(value) {
            const state = activeAudioData(value);
            return {format: state.format, sampleRate: state.rate,
                numberOfFrames: state.frames, numberOfChannels: state.channels,
                timestamp: state.timestamp, bytes: state.bytes};
        },
        chunkSnapshot(value) {
            const video = encodedVideoStates.has(value);
            const state = video ? encodedVideoState(value) : encodedAudioState(value);
            return {type: state.type, timestamp: state.timestamp,
                duration: state.duration, bytes: state.bytes, video};
        },
        receive(record, bytes) {
            try { return new AudioData({...record, data: bytes}); }
            catch (_) { throw frameError('Invalid AudioData clone record', 'DataCloneError'); }
        },
        receiveChunk(record, bytes) {
            try {
                const Constructor = record.video ? EncodedVideoChunk : EncodedAudioChunk;
                return new Constructor({...record, duration: record.duration ?? undefined, data: bytes});
            }
            catch (_) { throw frameError('Invalid EncodedAudioChunk clone record', 'DataCloneError'); }
        }
    };
    globalThis.__audioCloneBindings = audioCloneBindings;
    const installAudioMessageClone = () => { globalThis.__audioMessageCloneBindings = [
        value => audioDataStates.has(value) || encodedAudioStates.has(value) || encodedVideoStates.has(value),
        value => {
            const chunk = encodedAudioStates.has(value) || encodedVideoStates.has(value);
            if (!chunk && audioCloneBindings.closed(value))
                throw frameError('Closed AudioData cannot be cloned', 'DataCloneError');
            return {kind:chunk ? 'chunk' : 'data',
                value:chunk ? audioCloneBindings.chunkSnapshot(value) : audioCloneBindings.snapshot(value)};
        },
        record => {
            if (record.kind === 'data') return audioCloneBindings.receive(record.value, record.value.bytes);
            if (record.kind === 'chunk') return audioCloneBindings.receiveChunk(record.value, record.value.bytes);
            throw frameError('Unknown native audio clone kind', 'DataCloneError');
        },
        value => audioDataStates.has(value) && !audioCloneBindings.closed(value),
        value => { closeAudioData(value); return true; }
    ]; };
