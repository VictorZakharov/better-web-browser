    const audioContextToken = {};

    class BaseAudioContext extends EventTarget {
        constructor(token, channels, length, sampleRate) {
            super();
            if (token !== audioContextToken) throw new TypeError('Illegal constructor');
            audioContextState.set(this, { channels, length, sampleRate, time: 0,
                state: 'suspended', nodes: new Set(), connections: 0,
                renderStarted: false, onstatechange: null, oncomplete: null,
                suspensions: new Map(), advance: null, delayBytes: 0,
                sourceSnapshotBytes: 0, automationCurveBytes: 0 });
            Object.defineProperty(this, 'destination', { enumerable: true,
                value: new AudioDestinationNode(audioNodeToken, this) });
        }
        get sampleRate() { return audioContextState.get(this).sampleRate; }
        get currentTime() { return audioContextState.get(this).time; }
        get state() { return audioContextState.get(this).state; }
        get onstatechange() { return audioContextState.get(this).onstatechange; }
        set onstatechange(callback) {
            const state = audioContextState.get(this);
            if (state.onstatechange) this.removeEventListener('statechange', state.onstatechange);
            state.onstatechange = typeof callback === 'function' ? callback : null;
            if (state.onstatechange) this.addEventListener('statechange', state.onstatechange);
        }
        createBuffer(channels, length, rate) {
            return new AudioBuffer({ numberOfChannels: channels, length, sampleRate: rate });
        }
        createGain() { return new GainNode(this); }
        createOscillator() { return new OscillatorNode(this); }
        createBufferSource() { return new AudioBufferSourceNode(this); }
        createConstantSource() { return new ConstantSourceNode(this); }
        createStereoPanner() { return new StereoPannerNode(this); }
        createDelay(maxDelayTime = 1) { return new DelayNode(this, { maxDelayTime }); }
        createChannelSplitter(numberOfOutputs = 6) {
            return new ChannelSplitterNode(this, { numberOfOutputs });
        }
        createChannelMerger(numberOfInputs = 6) {
            return new ChannelMergerNode(this, { numberOfInputs });
        }
        createIIRFilter(feedforward, feedback) {
            return new IIRFilterNode(this, { feedforward, feedback });
        }
        createBiquadFilter() { return new BiquadFilterNode(this); }
        createWaveShaper() { return new WaveShaperNode(this); }
        createPeriodicWave(real, imag, constraints = {}) {
            if (real === undefined || imag === undefined)
                throw new TypeError('createPeriodicWave requires real and imag');
            return new PeriodicWave(this, { real, imag,
                disableNormalization: constraints.disableNormalization });
        }
        createAnalyser() { return new AnalyserNode(this); }
    }

    const setAudioContextState = (context, value) => {
        const state = audioContextState.get(context);
        if (state.state === value) return;
        state.state = value;
        audioTask(() => context.dispatchEvent(new AudioEvent('statechange')), 0);
    };

    class OfflineAudioCompletionEvent extends AudioEvent {
        constructor(type, options = {}) {
            super(type, options);
            if (!(options.renderedBuffer instanceof AudioBuffer))
                throw new TypeError('OfflineAudioCompletionEvent requires a rendered AudioBuffer');
            Object.defineProperty(this, 'renderedBuffer', { enumerable: true,
                value: options.renderedBuffer });
        }
    }

    const renderAudioNode = (context, node, frame, frames, cache, output = 0) => {
        const rendered = cache.get(node);
        if (rendered?.has(output)) return rendered.get(output);
        const samples = audioNodeState.get(node).render(frame, frames, cache, output);
        if (rendered) rendered.set(output, samples);
        else cache.set(node, new Map([[output, samples]]));
        return samples;
    };
    const mixAudioInputs = (context, node, frame, frames, cache, channels,
        inputIndex = 0, interpretation = 'speakers') => {
        const mixed = silence(channels, frames);
        for (const edge of audioNodeState.get(node).inputs[inputIndex]) {
            const samples = renderAudioNode(context, edge.source, frame, frames,
                cache, edge.output);
            for (let channel = 0; channel < channels; ++channel) {
                for (let i = 0; i < frames; ++i) {
                    let sample;
                    if (channels === 1 && samples.length > 1) {
                        sample = 0;
                        for (const source of samples) sample += source[i] / samples.length;
                    } else if (samples.length === 1 && interpretation !== 'explicit')
                        sample = samples[0][i];
                    else sample = samples[channel]?.[i] ?? 0;
                    mixed[channel][i] += sample;
                }
            }
        }
        return mixed;
    };
    const renderAudioQuantum = (context, frame, frames) => {
        const state = audioContextState.get(context);
        const cache = new Map();
        const samples = renderAudioNode(context, context.destination, frame, frames, cache);
        // Disconnected stateful nodes still advance once per quantum. A later
        // connection must not replay a source or erase a filter/delay tail.
        for (const node of state.nodes)
            if ((node instanceof AudioScheduledSourceNode || node instanceof DelayNode ||
                node instanceof IIRFilterNode || node instanceof BiquadFilterNode ||
                node instanceof AnalyserNode) && !cache.has(node))
                renderAudioNode(context, node, frame, frames, cache);
        return samples;
    };
    class OfflineAudioContext extends BaseAudioContext {
        constructor(channelsOrOptions, length, sampleRate) {
            const dictionary = typeof channelsOrOptions === 'object' &&
                channelsOrOptions !== null;
            const options = dictionary ?
                channelsOrOptions :
                { numberOfChannels: channelsOrOptions, length, sampleRate };
            const channels = validChannels(dictionary &&
                options.numberOfChannels === undefined ? 1 : options.numberOfChannels);
            const frames = validLength(options.length);
            const rate = validSampleRate(options.sampleRate);
            if (channels * frames * 4 > MAX_AUDIO_BUFFER_BYTES)
                throw new DOMException('Offline render exceeds the implementation limit', 'NotSupportedError');
            super(audioContextToken, channels, frames, rate);
        }
        get numberOfChannels() { return audioContextState.get(this).channels; }
        get length() { return audioContextState.get(this).length; }
        get oncomplete() { return audioContextState.get(this).oncomplete; }
        set oncomplete(callback) {
            const state = audioContextState.get(this);
            if (state.oncomplete) this.removeEventListener('complete', state.oncomplete);
            state.oncomplete = typeof callback === 'function' ? callback : null;
            if (state.oncomplete) this.addEventListener('complete', state.oncomplete);
        }
        suspend(suspendTime) {
            const state = audioContextState.get(this);
            const time = Number(suspendTime);
            const frame = Math.ceil(time * state.sampleRate / AUDIO_QUANTUM) * AUDIO_QUANTUM;
            if (!Number.isFinite(frame) || frame <= state.time * state.sampleRate ||
                frame >= state.length || state.suspensions.has(frame) || state.state === 'closed')
                return new AudioPromise((_, reject) => reject(new AudioDOMException(
                    'Invalid offline suspension time', 'InvalidStateError')));
            return new AudioPromise((resolve, reject) =>
                state.suspensions.set(frame, { resolve, reject }));
        }
        resume() {
            const state = audioContextState.get(this);
            if (!state.renderStarted || state.state === 'closed')
                return new AudioPromise((_, reject) => reject(new AudioDOMException(
                    'Offline rendering is not suspended', 'InvalidStateError')));
            if (state.state !== 'suspended')
                return new AudioPromise(resolve => resolve());
            setAudioContextState(this, 'running');
            return new AudioPromise(resolve => audioTask(() => {
                resolve();
                audioTask(state.advance);
            }));
        }
        startRendering() {
            const state = audioContextState.get(this);
            if (state.renderStarted)
                return new AudioPromise((_, reject) => reject(new AudioDOMException(
                    'Offline rendering already started', 'InvalidStateError')));
            // Bounds include graph work as well as the output buffer. A large graph cannot
            // monopolize one task merely because its output has few channels.
            if (!audioWorkWithinLimit(state, state.nodes.size))
                return new AudioPromise((_, reject) => reject(new AudioDOMException(
                    'Offline graph exceeds the rendering work limit', 'NotSupportedError')));
            state.renderStarted = true;
            setAudioContextState(this, 'running');
            return new AudioPromise((resolve, reject) => {
                const result = new AudioBuffer({ numberOfChannels: state.channels,
                    length: state.length, sampleRate: state.sampleRate });
                const resultData = audioBufferData.get(result);
                let frame = 0;
                const advance = () => {
                    try {
                        const nodes = state.nodes.size;
                        if (!audioWorkWithinLimit(state, nodes))
                            throw new AudioDOMException('Offline graph exceeds the rendering work limit',
                                'NotSupportedError');
                        // A graph may grow between tasks or while suspended. Refresh the
                        // yield cadence from its actual size on every advance.
                        const quantaPerTask = Math.max(1, Math.min(16,
                            Math.floor(65_536 / (AUDIO_QUANTUM * state.channels * nodes))));
                        for (let quantum = 0; quantum < quantaPerTask && frame < state.length;
                            ++quantum) {
                            const suspension = state.suspensions.get(frame);
                            if (suspension) {
                                state.suspensions.delete(frame);
                                setAudioContextState(this, 'suspended');
                                audioTask(suspension.resolve);
                                return;
                            }
                            const frames = Math.min(AUDIO_QUANTUM, state.length - frame);
                            const samples = renderAudioQuantum(this, frame, frames);
                            for (let channel = 0; channel < state.channels; ++channel)
                                resultData[channel].set(samples[channel], frame);
                            frame += frames;
                            state.time = frame / state.sampleRate;
                        }
                        if (frame < state.length) {
                            audioTask(advance, 0);
                            return;
                        }
                        setAudioContextState(this, 'closed');
                        // Completion crosses a media task boundary. The legacy
                        // complete event is queued only after resolving the promise.
                        audioTask(() => {
                            resolve(result);
                            audioTask(() => this.dispatchEvent(new OfflineAudioCompletionEvent(
                                'complete', { renderedBuffer: result })));
                        });
                    } catch (error) {
                        setAudioContextState(this, 'closed');
                        for (const suspension of state.suspensions.values())
                            suspension.reject(error);
                        state.suspensions.clear();
                        audioTask(() => reject(error));
                    }
                };
                state.advance = advance;
                audioTask(advance);
            });
        }
    }
