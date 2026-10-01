    // A captured track enters the existing Web Audio graph, not an independent
    // speaker voice. The capture callback only queues bounded PCM; graph pulls
    // exactly 128 frames when the node is connected to a live destination.
    const MAX_CAPTURE_AUDIO_PACKETS = 8;
    const audioCaptureSources = new WeakMap();
    const audioCaptureState = new WeakMap();
    const clearCaptureAudio = state => {
        state.packets.length = 0;
        state.offset = 0;
        state.lastSequence = null;
        state.lastTimestamp = null;
    };

    const captureGraphIsObserved = node => {
        if (audioNodeReaches(node, node.context.destination)) return true;
        // An analyser is pulled even when its branch is disconnected from the
        // speakers; microphone metering must still receive its input quanta.
        for (const candidate of audioContextState.get(node.context).nodes)
            if (candidate instanceof AnalyserNode && audioNodeReaches(node, candidate))
                return true;
        return false;
    };

    const captureSampleAt = (packets, position, channel) => {
        let remaining = position;
        for (const packet of packets) {
            if (remaining < packet.frames)
                return packet.samples[Math.floor(remaining) * packet.channels + channel] / 32768;
            remaining -= packet.frames;
        }
        return null;
    };

    const renderCaptureAudio = (node, frames) => {
        const state = audioCaptureState.get(node);
        const track = state.track;
        if (track.readyState !== 'live' || !track.enabled || track.muted) {
            clearCaptureAudio(state);
            return silence(1, frames);
        }
        const channels = state.packets[0]?.channels ?? state.channels;
        const output = silence(channels, frames);
        const contextRate = node.context.sampleRate;
        for (let frame = 0; frame < frames; ++frame) {
            const packet = state.packets[0];
            if (!packet) break;
            const position = state.offset;
            const whole = Math.floor(position);
            const fraction = position - whole;
            for (let channel = 0; channel < channels; ++channel) {
                const first = captureSampleAt(state.packets, whole, channel);
                if (first === null) break;
                const second = captureSampleAt(state.packets, whole + 1, channel);
                output[channel][frame] = first + ((second ?? first) - first) * fraction;
            }
            state.offset += packet.rate / contextRate;
            while (state.packets.length && state.offset >= state.packets[0].frames) {
                state.offset -= state.packets[0].frames;
                state.packets.shift();
            }
            if (!state.packets.length) state.offset = 0;
        }
        return output;
    };

    class MediaStreamAudioSourceNode extends AudioNode {
        constructor(context, options) {
            if (!(context instanceof AudioContext))
                throw new TypeError('MediaStreamAudioSourceNode requires an AudioContext');
            // MediaStreamAudioSourceOptions is not an AudioNodeOptions subtype.
            options = audioOptionsDictionary(context, options);
            const stream = options.mediaStream;
            if (typeof MediaStream !== 'function' || !(stream instanceof MediaStream))
                throw new TypeError('mediaStream must be a MediaStream');
            // Web Audio 1.1 §1.24 fixes the first audio track in code-unit ID
            // order at construction; later MediaStream removals do not retarget.
            const tracks = stream.getAudioTracks().sort((a, b) =>
                a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
            if (!tracks.length)
                throw new DOMException('MediaStream has no audio track', 'InvalidStateError');
            super(audioNodeToken, context, 0, 1);
            Object.defineProperty(this, 'mediaStream', { enumerable: true, value: stream });
            const state = { track: tracks[0], packets: [], offset: 0,
                channels: 1, lastSequence: null, lastTimestamp: null,
                lastFrames: 0, lastRate: 0 };
            audioCaptureState.set(this, state);
            audioNodeState.get(this).render = (_frame, frames) =>
                renderCaptureAudio(this, frames);
            let sources = audioCaptureSources.get(stream);
            if (!sources) {
                sources = new Set();
                audioCaptureSources.set(stream, sources);
            }
            sources.add(this);
            tracks[0].addEventListener('ended', () => clearCaptureAudio(state));
        }
    }

    const receiveCaptureAudio = (stream, sequence, timestamp, rate, channels, frames, bytes) => {
        const sources = audioCaptureSources.get(stream);
        if (!sources?.size) return;
        if (!Number.isSafeInteger(sequence) || sequence < 1 ||
            !Number.isSafeInteger(timestamp) || timestamp < 0 ||
            !Number.isInteger(rate) || rate < 8000 || rate > 48000 ||
            !Number.isInteger(channels) || channels < 1 || channels > 2 ||
            !Number.isInteger(frames) || frames < 1 || frames > 960 ||
            !(bytes instanceof Uint8Array) || bytes.byteLength !== frames * channels * 2)
            return;
        // Copy once, because the renderer-owned typed array is not an audio
        // graph snapshot and could otherwise be changed by another callback.
        const samples = new Int16Array(frames * channels);
        const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
        for (let index = 0; index < samples.length; ++index)
            samples[index] = view.getInt16(index * 2, true);
        const packet = { sequence, timestamp, rate, channels, frames, samples };
        for (const node of sources) {
            const state = audioCaptureState.get(node);
            if (node.context.state === 'closed' || state.track.readyState !== 'live' ||
                !state.track.enabled || state.track.muted ||
                !captureGraphIsObserved(node)) {
                clearCaptureAudio(state);
                continue;
            }
            if (state.lastSequence !== null && sequence <= state.lastSequence) continue;
            // A dropped packet must not be played as though it were adjacent
            // to its successor. Reset on a sequence or timestamp discontinuity.
            const expected = state.lastTimestamp === null ? timestamp :
                state.lastTimestamp + state.lastFrames * 10_000_000 / state.lastRate;
            if (state.lastSequence !== null &&
                (sequence !== state.lastSequence + 1 ||
                    timestamp < state.lastTimestamp || timestamp > expected + 1_000_000 ||
                    state.lastRate !== rate || state.channels !== channels))
                clearCaptureAudio(state);
            state.lastSequence = sequence;
            state.lastTimestamp = timestamp;
            state.lastFrames = frames;
            state.lastRate = rate;
            if (state.packets.length === MAX_CAPTURE_AUDIO_PACKETS) {
                state.packets.shift();
                state.offset = 0;
            }
            state.packets.push(packet);
            state.channels = channels;
        }
    };

    const clearCaptureAudioForStream = stream => {
        for (const node of audioCaptureSources.get(stream) ?? [])
            clearCaptureAudio(audioCaptureState.get(node));
    };
    const clearCaptureAudioForContext = context => {
        for (const node of audioContextState.get(context).nodes)
            if (node instanceof MediaStreamAudioSourceNode) {
                clearCaptureAudio(audioCaptureState.get(node));
                audioCaptureSources.get(node.mediaStream)?.delete(node);
            }
    };
    Object.defineProperties(globalThis, {
        __webAudioCaptureFrame: { value: receiveCaptureAudio },
        __webAudioCaptureStopped: { value: clearCaptureAudioForStream }
    });
