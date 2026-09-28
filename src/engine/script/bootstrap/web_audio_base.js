// Web Audio's offline and live renderers share this consecutively concatenated graph.
(() => {
    'use strict';
    // The Web Audio rendering quantum is fixed at 128 sample frames.
    const AUDIO_QUANTUM = 128;
    const MAX_AUDIO_BUFFER_BYTES = 16 * 1024 * 1024;
    const MAX_AUDIO_NODES = 256;
    const MAX_AUDIO_CONNECTIONS = 512;
    const MAX_AUDIO_DELAY_BYTES = 32 * 1024 * 1024;
    const MAX_AUDIO_SOURCE_SNAPSHOT_BYTES = 64 * 1024 * 1024;
    const MAX_AUDIO_AUTOMATION_CURVE_BYTES = 32 * 1024 * 1024;
    const audioWorkWithinLimit = (state, nodes) =>
        (!Number.isFinite(state.length) ||
            state.length * state.channels * nodes <= 64_000_000) &&
        AUDIO_QUANTUM * state.channels * nodes <= 262_144;
    const audioContextState = new WeakMap();
    const audioTask = globalThis.__webAudioRenderTask;
    const audioHost = globalThis.__webAudioHostCall;
    delete globalThis.__webAudioRenderTask;
    delete globalThis.__webAudioHostCall;
    const AudioPromise = globalThis.Promise;
    const AudioEvent = globalThis.Event;
    const AudioDOMException = globalThis.DOMException;

    const finite = (value, name) => {
        value = Number(value);
        if (!Number.isFinite(value)) throw new TypeError(name + ' must be finite');
        return value;
    };
    const finiteFloat = (value, name) => {
        const converted = Math.fround(finite(value, name));
        if (!Number.isFinite(converted))
            throw new TypeError(name + ' is outside the float range');
        return converted;
    };
    const positiveFloat = (value, name) => {
        value = finiteFloat(value, name);
        if (value <= 0) throw new RangeError(name + ' must be positive');
        return value;
    };
    const nonnegative = (value, name) => {
        value = finite(value, name);
        if (value < 0) throw new RangeError(name + ' must be nonnegative');
        return value;
    };
    const positive = (value, name) => {
        value = finite(value, name);
        if (value <= 0) throw new RangeError(name + ' must be positive');
        return value;
    };
    const validSampleRate = value => {
        value = positive(value, 'sampleRate');
        if (value < 8000 || value > 192000)
            throw new DOMException('Unsupported sample rate', 'NotSupportedError');
        return value;
    };
    const validChannels = value => {
        value = Number(value);
        if (!Number.isInteger(value) || value < 1 || value > 32)
            throw new DOMException('Unsupported channel count', 'NotSupportedError');
        return value;
    };
    const validLength = value => {
        value = Number(value);
        if (!Number.isInteger(value) || value < 1)
            throw new DOMException('Invalid buffer length', 'NotSupportedError');
        return value;
    };
    const silence = (channels, frames) =>
        Array.from({ length: channels }, () => new Float32Array(frames));
