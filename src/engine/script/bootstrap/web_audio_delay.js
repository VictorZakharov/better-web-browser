    // Delay storage retains a layout alongside each frame (§4.3). A new input
    // layout reaches the output only when those delayed frames arrive.
    const audioDelayState = new WeakMap();
    class DelayNode extends AudioNode {
        constructor(context, options = {}) {
            const channels = readAudioChannelOptions(context, options);
            options = channels.options;
            const initial = audioOption(options, 'delayTime', 0,
                value => finite(value, 'delayTime'));
            const maxDelayTime = audioOption(options, 'maxDelayTime', 1,
                value => finite(value, 'maxDelayTime'));
            if (maxDelayTime <= 0 || maxDelayTime >= 180)
                throw new AudioDOMException('maxDelayTime must be greater than 0 and less than 180',
                    'NotSupportedError');
            const settings = audioChannelSettings(context, channels);
            const contextState = audioContextState.get(context);
            // A cyclic delay is clamped to one quantum even when the supplied
            // maxDelayTime is shorter (§1.18.4).
            const ringLength = Math.max(AUDIO_QUANTUM,
                Math.ceil(maxDelayTime * contextState.sampleRate)) + 2;
            const count = settings.channelCountMode === 'explicit' ? settings.channelCount : 1;
            const requiredBytes = ringLength * (4 * count + 1);
            if (requiredBytes > MAX_AUDIO_DELAY_BYTES - contextState.delayBytes)
                throw new DOMException('Audio delay lines exceed the context memory limit',
                    'NotSupportedError');
            super(audioNodeToken, context, 1, 1, settings);
            const delayTime = new AudioParam(audioParamToken, context, initial,
                0, maxDelayTime);
            Object.defineProperty(this, 'delayTime', { enumerable: true,
                value: delayTime });
            audioParamState.get(delayTime).owner = this;
            contextState.delayBytes += requiredBytes;
            audioDelayState.set(this, { ring: silence(count, ringLength), ringLength,
                layouts: new Uint8Array(ringLength), written: 0, maxDelayTime });
            audioNodeState.get(this).render = (frame, frames, cache) =>
                renderDelay(this, frame, frames, cache);
        }
    }
    const growDelayChannels = (node, channels) => {
        const state = audioDelayState.get(node);
        if (channels <= state.ring.length) return;
        const context = audioContextState.get(node.context);
        const bytes = (channels - state.ring.length) * state.ringLength * 4;
        if (bytes > MAX_AUDIO_DELAY_BYTES - context.delayBytes)
            throw new AudioDOMException('Audio delay channel growth exceeds the memory limit',
                'NotSupportedError');
        // Allocate before changing ownership accounting. Retired channels keep
        // bounded history until the circular writer has overwritten it.
        const added = silence(channels - state.ring.length, state.ringLength);
        state.ring.push(...added);
        context.delayBytes += bytes;
    };
    const prepareDelayReader = (node, frame, frames, cache) => {
        growDelayChannels(node, cache.plan.widths.get(node));
    };
    const writeDelayFrame = (state, input, offset, frame) => {
        const index = frame % state.ringLength;
        state.layouts[index] = input.length;
        for (let channel = 0; channel < state.ring.length; ++channel)
            state.ring[channel][index] = input[channel]?.[offset] ?? 0;
    };
    const delayFrameValue = (state, frame, channel) => {
        if (frame < 0 || frame < state.written - state.ringLength) return 0;
        return state.ring[channel]?.[frame % state.ringLength] ?? 0;
    };
    const delayFrameWidth = (state, frame) => frame < 0 ? 1 :
        state.layouts[frame % state.ringLength] || 1;
    const delayReadPosition = (node, state, frame, offset, cache, cyclic) => {
        const seconds = Math.min(state.maxDelayTime, Math.max(0,
            audioParamValueForFrame(node.delayTime, frame, offset,
                node.context.sampleRate, cache)));
        return frame + offset - Math.max(cyclic ? AUDIO_QUANTUM : 0,
            seconds * node.context.sampleRate);
    };
    const renderDelay = (node, frame, frames, cache) => {
        const state = audioDelayState.get(node);
        const cyclic = cache.plan.delays.has(node);
        const input = cyclic ? null : mixAudioInputs(node.context, node, frame, frames, cache);
        if (input) growDelayChannels(node, input.length);
        const fractions = new Float64Array(frames);
        const widths = new Uint8Array(frames * 2);
        const first = silence(state.ring.length, frames);
        const second = silence(state.ring.length, frames);
        let channels = 1;
        // Write immediately before each non-cyclic read. Writing a whole block
        // first would overwrite short-delay samples when the ring wraps.
        for (let i = 0; i < frames; ++i) {
            if (input) {
                writeDelayFrame(state, input, i, frame + i);
                state.written = frame + i + 1;
            }
            const position = delayReadPosition(node, state, frame, i, cache, cyclic);
            const before = Math.floor(position), fraction = position - before;
            const firstWidth = delayFrameWidth(state, before);
            const secondWidth = fraction ? delayFrameWidth(state, before + 1) : firstWidth;
            fractions[i] = fraction;
            widths[i * 2] = firstWidth;
            widths[i * 2 + 1] = secondWidth;
            channels = Math.max(channels, firstWidth, secondWidth);
            for (let channel = 0; channel < first.length; ++channel) {
                first[channel][i] = delayFrameValue(state, before, channel);
                second[channel][i] = fraction ?
                    delayFrameValue(state, before + 1, channel) : first[channel][i];
            }
        }
        return mixDelayFrames(node, first, second, fractions, widths, channels, frames);
    };
    const mixDelayFrames = (node, first, second, fractions, widths, channels, frames) => {
        const output = silence(channels, frames);
        const interpretation = audioNodeState.get(node).channelInterpretation;
        // A fractional read straddling a layout transition must map both
        // historical buses before interpolation, not infer a surround layout.
        for (let i = 0; i < frames; ++i) {
            const a = audioChannelMatrix(widths[i * 2], channels, interpretation);
            const b = audioChannelMatrix(widths[i * 2 + 1], channels, interpretation);
            for (let channel = 0; channel < channels; ++channel) {
                let left = 0, right = 0;
                for (const [source, gain] of a[channel]) left += first[source][i] * gain;
                for (const [source, gain] of b[channel]) right += second[source][i] * gain;
                output[channel][i] = left + (right - left) * fractions[i];
            }
        }
        return output;
    };
    const writeDelayQuantum = (node, frame, frames, cache) => {
        const state = audioDelayState.get(node);
        const input = mixAudioInputs(node.context, node, frame, frames, cache);
        growDelayChannels(node, input.length);
        for (let i = 0; i < frames; ++i) writeDelayFrame(state, input, i, frame + i);
        state.written = frame + frames;
    };
