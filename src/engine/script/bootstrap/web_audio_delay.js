    // A bounded delay line. The reader indexes the previous input stream by
    // absolute sample frame, so automation can change delayTime every sample.
    const audioDelayState = new WeakMap();
    class DelayNode extends AudioNode {
        constructor(context, options = {}) {
            const maxDelayTime = Number(options.maxDelayTime === undefined ? 1 :
                options.maxDelayTime);
            if (!Number.isFinite(maxDelayTime) || maxDelayTime <= 0 || maxDelayTime >= 180)
                throw new DOMException('maxDelayTime must be greater than 0 and less than 180',
                    'NotSupportedError');
            const initial = finite(options.delayTime === undefined ? 0 :
                options.delayTime, 'delayTime');
            const contextState = audioContextState.get(context);
            if (!contextState) throw new TypeError('DelayNode requires an audio context');
            const ringLength = Math.ceil(maxDelayTime * contextState.sampleRate) + 2;
            const requiredBytes = ringLength * contextState.channels * 4;
            if (requiredBytes > MAX_AUDIO_DELAY_BYTES - contextState.delayBytes)
                throw new DOMException('Audio delay lines exceed the context memory limit',
                    'NotSupportedError');
            super(audioNodeToken, context, 1, 1);
            const delayTime = new AudioParam(audioParamToken, context, initial,
                0, maxDelayTime);
            Object.defineProperty(this, 'delayTime', { enumerable: true,
                value: delayTime });
            audioParamState.get(delayTime).owner = this;
            contextState.delayBytes += requiredBytes;
            audioDelayState.set(this, { ring: Array.from({ length: contextState.channels },
                () => new Float32Array(ringLength)), ringLength, written: 0, maxDelayTime });
            audioNodeState.get(this).render = (frame, frames, cache) =>
                renderDelay(this, frame, frames, cache);
        }
    }
    const renderDelay = (node, frame, frames, cache) => {
        const state = audioDelayState.get(node);
        const channels = audioContextState.get(node.context).channels;
        const input = mixAudioInputs(node.context, node, frame, frames, cache, channels);
        const output = silence(channels, frames);
        const rate = node.context.sampleRate;
        for (let i = 0; i < frames; ++i) {
            const written = state.written++;
            const writeIndex = written % state.ringLength;
            const delay = Math.min(state.maxDelayTime, Math.max(0,
                audioParamValueForFrame(node.delayTime, frame, i, rate, cache))) * rate;
            const readFrame = written - delay;
            const before = Math.floor(readFrame);
            const fraction = readFrame - before;
            for (let channel = 0; channel < channels; ++channel) {
                const ring = state.ring[channel];
                ring[writeIndex] = input[channel][i];
                const first = before < 0 ? 0 : ring[before % state.ringLength];
                const second = fraction === 0 || before + 1 < 0 ? 0 :
                    ring[(before + 1) % state.ringLength];
                output[channel][i] = first + (second - first) * fraction;
            }
        }
        return output;
    };
