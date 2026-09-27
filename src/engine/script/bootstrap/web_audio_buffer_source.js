    const audioBufferSourceState = new WeakMap();
    const releaseBufferSourceContent = node => {
        const state = audioBufferSourceState.get(node);
        if (!state.acquired) return;
        audioContextState.get(node.context).sourceSnapshotBytes -= state.acquired.bytes;
        state.acquired = null;
    };
    const acquireBufferSourceContent = (node, buffer) => {
        const context = audioContextState.get(node.context);
        const metadata = audioBufferInfo(buffer);
        const bytes = metadata.numberOfChannels * metadata.length * 4;
        if (bytes > MAX_AUDIO_SOURCE_SNAPSHOT_BYTES - context.sourceSnapshotBytes)
            throw new DOMException('Audio source snapshots exceed the implementation limit',
                'NotSupportedError');
        // Web Audio 1.0 §1.4.3 acquires immutable channel content at start (or
        // when a buffer is first assigned to an already-started source).
        const data = audioBufferData.get(buffer).map(channel => new Float32Array(channel));
        context.sourceSnapshotBytes += bytes;
        audioBufferSourceState.get(node).acquired = { ...metadata, data, bytes };
    };
    const finishBufferSource = node => {
        releaseBufferSourceContent(node);
        finishAudioSource(node);
    };

    class AudioBufferSourceNode extends AudioScheduledSourceNode {
        constructor(context, options = {}) {
            const suppliedBuffer = options.buffer;
            const buffer = suppliedBuffer === undefined ? null : suppliedBuffer;
            if (buffer !== null && !(buffer instanceof AudioBuffer))
                throw new TypeError('Buffer must be an AudioBuffer or null');
            const loop = !!options.loop;
            const suppliedLoopStart = options.loopStart;
            const loopStart = suppliedLoopStart === undefined ? 0 :
                nonnegative(suppliedLoopStart, 'loopStart');
            const suppliedLoopEnd = options.loopEnd;
            const loopEnd = suppliedLoopEnd === undefined ? 0 :
                nonnegative(suppliedLoopEnd, 'loopEnd');
            const suppliedPlaybackRate = options.playbackRate;
            const playbackRate = suppliedPlaybackRate === undefined ? 1 :
                finiteFloat(suppliedPlaybackRate, 'playbackRate');
            const suppliedDetune = options.detune;
            const detune = suppliedDetune === undefined ? 0 :
                finiteFloat(suppliedDetune, 'detune');
            super(audioNodeToken, context);
            audioBufferSourceState.set(this, { buffer: null, bufferSet: false,
                loop: false, loopStart: 0, loopEnd: 0, offset: 0,
                duration: Infinity, contentPlayed: 0, position: 0, acquired: null });
            this.buffer = buffer;
            this.loop = loop;
            this.loopStart = loopStart;
            this.loopEnd = loopEnd;
            Object.defineProperties(this, {
                playbackRate: { enumerable: true, value: new AudioParam(audioParamToken, context,
                    playbackRate,
                    -3.4028234663852886e38, 3.4028234663852886e38, 'k-rate', true) },
                detune: { enumerable: true, value: new AudioParam(audioParamToken, context,
                    detune,
                    -153600, 153600, 'k-rate', true) }
            });
            audioParamState.get(this.playbackRate).owner = this;
            audioParamState.get(this.detune).owner = this;
            audioNodeState.get(this).render = (frame, frames, cache) =>
                renderBufferSource(this, frame, frames, cache);
        }
        get buffer() { return audioBufferSourceState.get(this).buffer; }
        set buffer(value) {
            if (value !== null && !(value instanceof AudioBuffer))
                throw new TypeError('Buffer must be an AudioBuffer or null');
            const state = audioBufferSourceState.get(this);
            if (value !== null && state.bufferSet)
                throw new DOMException('AudioBufferSourceNode buffer was already set',
                    'InvalidStateError');
            if (value !== null && audioScheduledState.get(this).start !== null) {
                acquireBufferSourceContent(this, value);
                state.position = Math.min(state.offset, value.duration) * value.sampleRate;
            }
            if (value === null && audioScheduledState.get(this).start !== null)
                releaseBufferSourceContent(this);
            if (value !== null) state.bufferSet = true;
            state.buffer = value;
        }
        get loop() { return audioBufferSourceState.get(this).loop; }
        set loop(value) { audioBufferSourceState.get(this).loop = !!value; }
        get loopStart() { return audioBufferSourceState.get(this).loopStart; }
        set loopStart(value) {
            audioBufferSourceState.get(this).loopStart = nonnegative(value, 'loopStart');
        }
        get loopEnd() { return audioBufferSourceState.get(this).loopEnd; }
        set loopEnd(value) {
            audioBufferSourceState.get(this).loopEnd = nonnegative(value, 'loopEnd');
        }
        start(when = 0, offset = 0, duration) {
            when = nonnegative(when, 'when');
            offset = nonnegative(offset, 'offset');
            if (duration !== undefined) duration = nonnegative(duration, 'duration');
            if (audioScheduledState.get(this).start !== null)
                throw new DOMException('Source has already started', 'InvalidStateError');
            const state = audioBufferSourceState.get(this);
            if (state.buffer) acquireBufferSourceContent(this, state.buffer);
            try { super.start(when); }
            catch (error) {
                releaseBufferSourceContent(this);
                throw error;
            }
            state.offset = offset;
            state.duration = duration === undefined ? Infinity : duration;
            state.contentPlayed = 0;
            state.position = this.buffer ? Math.min(offset, this.buffer.duration) *
                this.buffer.sampleRate : 0;
        }
    }
    const renderBufferSource = (node, frame, frames, cache) => {
        const state = audioBufferSourceState.get(node);
        const schedule = audioScheduledState.get(node);
        const buffer = state.acquired;
        const output = silence(buffer?.numberOfChannels || 1, frames);
        if (schedule.start === null) return output;
        for (let i = 0; i < frames; ++i) {
            const time = (frame + i) / node.context.sampleRate;
            if (time < schedule.start) continue;
            if (time >= schedule.stop || !buffer) { finishBufferSource(node); break; }
            if (state.contentPlayed >= state.duration) {
                finishBufferSource(node); break;
            }
            let loopStart = Math.min(buffer.length, state.loopStart * buffer.sampleRate);
            let loopEnd = state.loopEnd > 0 ?
                Math.min(buffer.length, state.loopEnd * buffer.sampleRate) : buffer.length;
            // An invalid loop interval covers the whole buffer, per Web Audio 1.0 §1.9.5.
            if (loopEnd <= loopStart) { loopStart = 0; loopEnd = buffer.length; }
            const rate = audioParamValueForFrame(node.playbackRate, frame, i,
                node.context.sampleRate, cache) * Math.pow(2,
                audioParamValueForFrame(node.detune, frame, i,
                    node.context.sampleRate, cache) / 1200);
            if (state.loop && rate >= 0 && state.position >= loopEnd)
                state.position = loopStart + (state.position - loopStart) % (loopEnd - loopStart);
            if (state.loop && rate < 0 && state.position < loopStart)
                state.position = loopEnd - (loopStart - state.position) % (loopEnd - loopStart);
            if (state.position < 0 || state.position >= buffer.length) {
                finishBufferSource(node); break;
            }
            const first = Math.floor(state.position);
            const atLoopEnd = state.loop && first + 1 >= loopEnd &&
                state.position < loopEnd;
            const fraction = atLoopEnd ?
                (state.position - first) / (loopEnd - first) : state.position - first;
            const second = first + 1 < buffer.length ? first + 1 : first;
            for (let channel = 0; channel < output.length; ++channel) {
                const samples = buffer.data[channel];
                const next = atLoopEnd ? samples[Math.floor(loopStart)] +
                    ((samples[Math.min(Math.floor(loopStart) + 1, buffer.length - 1)] -
                    samples[Math.floor(loopStart)]) * (loopStart % 1)) : samples[second];
                output[channel][i] = samples[first] + (next - samples[first]) * fraction;
            }
            state.position += rate * buffer.sampleRate / node.context.sampleRate;
            state.contentPlayed += Math.abs(rate) / node.context.sampleRate;
        }
        return output;
    };
