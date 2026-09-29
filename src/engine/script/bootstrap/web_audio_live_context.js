    // The renderer owns at most one live graph stream per document. A stream ID
    // is new on every resume because the media worker rejects retired IDs.
    let liveAudioContext = null;
    let nextLiveStreamId = 1;
    const MAX_LIVE_CHUNK_QUANTA = 7;
    const liveError = (name, message) => new AudioDOMException(message, name);
    const settleLivePromises = (promises, error) => {
        for (const promise of promises.splice(0)) {
            if (error) promise.reject(error);
            else promise.resolve();
        }
    };
    const scheduleLiveSend = (context, delay = 0) => {
        const live = audioContextState.get(context).live;
        if (live.sendScheduled || live.phase !== 'active') return;
        live.sendScheduled = true;
        audioTask(() => {
            live.sendScheduled = false;
            sendLiveChunk(context);
        }, delay);
    };
    const renderLiveChunk = context => {
        const state = audioContextState.get(context);
        const live = state.live;
        if (!audioWorkWithinLimit(state, state.nodes.size))
            throw liveError('NotSupportedError', 'Audio graph exceeds the rendering work limit');
        const frames = AUDIO_QUANTUM * live.chunkQuanta;
        const bytes = new Uint8Array(frames * state.channels * 2);
        const view = new DataView(bytes.buffer);
        for (let quantum = 0; quantum < live.chunkQuanta; ++quantum) {
            const samples = renderAudioQuantum(context, live.renderFrame, AUDIO_QUANTUM);
            for (let frame = 0; frame < AUDIO_QUANTUM; ++frame) {
                for (let channel = 0; channel < state.channels; ++channel) {
                    const value = samples[channel][frame];
                    const clipped = Number.isFinite(value) ? Math.max(-1, Math.min(1, value)) : 0;
                    const pcm = Math.round(clipped * (clipped < 0 ? 32768 : 32767));
                    view.setInt16(((quantum * AUDIO_QUANTUM + frame) * state.channels + channel) * 2,
                        pcm, true);
                }
            }
            live.renderFrame += AUDIO_QUANTUM;
        }
        return { bytes, frames, submitted: false };
    };
    const sendLiveChunk = context => {
        const state = audioContextState.get(context);
        const live = state.live;
        if (live.phase !== 'active' || live.pending?.submitted) return;
        try {
            // Retain the exact PCM and DSP state after local refusal. Rendering
            // the same quantum twice would duplicate source/automation progress.
            live.pending ??= renderLiveChunk(context);
            if (!audioHost('audioGraphQueue', live.streamId, state.sampleRate,
                state.channels, live.pending.bytes)) {
                scheduleLiveSend(context, 10);
                return;
            }
            live.pending.submitted = true;
        } catch (error) {
            live.userSuspended = true;
            live.finalClose = true;
            settleLivePromises(live.resumePromises, error);
            console.error('Web Audio graph stopped: ' + error);
            stopLiveStream(context);
        }
    };
    const scheduleLiveClose = context => {
        const live = audioContextState.get(context).live;
        if (live.phase !== 'closing' || live.closeSubmitted || live.closeScheduled) return;
        live.closeScheduled = true;
        audioTask(() => {
            live.closeScheduled = false;
            if (live.phase !== 'closing' || live.closeSubmitted) return;
            if (audioHost('audioGraphClose', live.streamId)) live.closeSubmitted = true;
            else scheduleLiveClose(context);
        }, 1);
    };
    const finishLiveClose = context => {
        const state = audioContextState.get(context);
        const live = state.live;
        live.phase = 'idle';
        live.streamId = 0;
        live.closeSubmitted = false;
        if (live.pending) live.pending.submitted = false;
        if (live.finalClose) {
            clearCaptureAudioForContext(context);
            live.pending = null;
            setAudioContextState(context, 'closed');
            settleLivePromises(live.closePromises);
            settleLivePromises(live.suspendPromises);
            liveAudioContext = null;
            return;
        }
        setAudioContextState(context, 'suspended');
        settleLivePromises(live.suspendPromises);
        if (!live.userSuspended) tryStartLive(context);
    };
    const stopLiveStream = context => {
        const live = audioContextState.get(context).live;
        if (live.phase === 'idle') {
            audioTask(() => finishLiveClose(context));
        } else if (live.phase === 'active') {
            live.phase = 'closing';
            scheduleLiveClose(context);
        }
    };
    const tryStartLive = context => {
        const state = audioContextState.get(context);
        const live = state.live;
        if (live.phase !== 'idle' || live.finalClose || live.userSuspended ||
            !audioHost('audioContextAllowed')) return;
        if (nextLiveStreamId > 0xffffffff) {
            settleLivePromises(live.resumePromises,
                liveError('NotSupportedError', 'Audio stream ID limit reached'));
            return;
        }
        live.streamId = nextLiveStreamId++;
        live.phase = 'active';
        live.acceptedChunks = 0;
        live.nextDeadline = 0;
        state.renderStarted = true;
        scheduleLiveSend(context);
    };

    class AudioContext extends BaseAudioContext {
        constructor(options = {}) {
            options = options == null ? {} : Object(options);
            const sampleRate = options.sampleRate === undefined ? 48000 :
                validSampleRate(options.sampleRate);
            // Above 48 kHz, a maximum-size transport chunk is shorter than
            // the native backpressure retry interval and cannot be paced reliably.
            if (!Number.isInteger(sampleRate) || sampleRate > 48000)
                throw liveError('NotSupportedError',
                    'Live audio sample rate must be an integer from 8000 to 48000 Hz');
            if (liveAudioContext && liveAudioContext.state !== 'closed')
                throw liveError('NotSupportedError', 'Only one live AudioContext is supported per document');
            super(audioContextToken, 2, Infinity, sampleRate);
            audioContextState.get(this).live = {
                phase: 'idle', streamId: 0, renderFrame: 0, acceptedFrame: 0,
                pending: null, sendScheduled: false, closeScheduled: false,
                closeSubmitted: false, userSuspended: false, finalClose: false,
                chunkQuanta: Math.max(1, Math.min(MAX_LIVE_CHUNK_QUANTA,
                    Math.floor(sampleRate / (AUDIO_QUANTUM * 50)))),
                acceptedChunks: 0, nextDeadline: 0,
                resumePromises: [], suspendPromises: [], closePromises: []
            };
            liveAudioContext = this;
            // Constructor auto-start, when policy permits, follows the current
            // script task so graph setup can happen before the first quantum.
            audioTask(() => tryStartLive(this));
        }
        createMediaStreamSource(stream) {
            return new MediaStreamAudioSourceNode(this, { mediaStream: stream });
        }
        resume() {
            const state = audioContextState.get(this);
            const live = state.live;
            if (live.finalClose || state.state === 'closed')
                return new AudioPromise((_, reject) => reject(liveError(
                    'InvalidStateError', 'AudioContext has been closed')));
            live.userSuspended = false;
            if (state.state === 'running' && live.phase === 'active')
                return new AudioPromise(resolve => resolve());
            return new AudioPromise((resolve, reject) => {
                live.resumePromises.push({ resolve, reject });
                tryStartLive(this);
            });
        }
        suspend() {
            const state = audioContextState.get(this);
            const live = state.live;
            if (live.finalClose || state.state === 'closed')
                return new AudioPromise((_, reject) => reject(liveError(
                    'InvalidStateError', 'AudioContext has been closed')));
            live.userSuspended = true;
            settleLivePromises(live.resumePromises,
                liveError('AbortError', 'AudioContext was suspended before playback started'));
            if (live.phase === 'idle') return new AudioPromise(resolve => resolve());
            return new AudioPromise(resolve => {
                live.suspendPromises.push({ resolve });
                stopLiveStream(this);
            });
        }
        close() {
            const state = audioContextState.get(this);
            const live = state.live;
            if (live.finalClose || state.state === 'closed')
                return new AudioPromise((_, reject) => reject(liveError(
                    'InvalidStateError', 'AudioContext has been closed')));
            live.finalClose = true;
            clearCaptureAudioForContext(this);
            settleLivePromises(live.resumePromises,
                liveError('AbortError', 'AudioContext was closed before playback started'));
            return new AudioPromise(resolve => {
                live.closePromises.push({ resolve });
                stopLiveStream(this);
            });
        }
    }

    Object.defineProperties(globalThis, {
        __notifyAudioActivation: { value: () => {
            if (liveAudioContext) tryStartLive(liveAudioContext);
        } },
        __receiveAudioGraphStatus: { value: (streamId, status) => {
            const context = liveAudioContext;
            if (!context) return;
            const state = audioContextState.get(context);
            const live = state.live;
            if (live.streamId !== streamId || live.phase === 'idle') return;
            if (status === 'accepted' && live.pending?.submitted) {
                live.acceptedFrame += live.pending.frames;
                state.time = live.acceptedFrame / state.sampleRate;
                const chunkDuration = live.pending.frames * 1000 / state.sampleRate;
                live.pending = null;
                if (live.phase === 'active') {
                    setAudioContextState(context, 'running');
                    settleLivePromises(live.resumePromises);
                    live.acceptedChunks++;
                    // Prime four transport chunks, then pace to the audio clock.
                    // The supported live rate keeps every chunk above 10 ms.
                    let delay = 0;
                    if (live.acceptedChunks >= 4) {
                        const now = performance.now();
                        live.nextDeadline = live.acceptedChunks === 4 ?
                            now + chunkDuration : live.nextDeadline + chunkDuration;
                        delay = Math.max(0, live.nextDeadline - now);
                    }
                    scheduleLiveSend(context, delay);
                }
            } else if (status === 'rejected' && live.phase === 'active') {
                live.userSuspended = true;
                settleLivePromises(live.resumePromises,
                    liveError('NotSupportedError', 'The audio output rejected this graph stream'));
                stopLiveStream(context);
            } else if (status === 'closed') {
                if (live.phase === 'active') {
                    live.userSuspended = true;
                    settleLivePromises(live.resumePromises,
                        liveError('NotSupportedError', 'The audio output closed this graph stream'));
                }
                finishLiveClose(context);
            }
        } }
    });
    Object.assign(globalThis, {
        AudioBuffer, AudioParam, AudioNode, AudioScheduledSourceNode,
        AudioDestinationNode, GainNode, OscillatorNode, AudioBufferSourceNode,
        ConstantSourceNode, StereoPannerNode, DelayNode,
        MediaStreamAudioSourceNode,
        ChannelSplitterNode, ChannelMergerNode, IIRFilterNode, BiquadFilterNode,
        DynamicsCompressorNode, ConvolverNode,
        WaveShaperNode, PeriodicWave, AnalyserNode,
        BaseAudioContext, AudioContext, OfflineAudioContext, OfflineAudioCompletionEvent
    });
})();
