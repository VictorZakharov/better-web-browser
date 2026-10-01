    const audioNodeToken = {};
    const audioNodeState = new WeakMap();
    const audioScheduledState = new WeakMap();
    const audioOscillatorState = new WeakMap();
    const audioNodeSuccessors = node => node instanceof AudioListener ?
        [...audioContextState.get(audioListenerContext.get(node)).panners] :
        audioNodeState.get(node).outputs.flatMap(port =>
            [...port].map(edge => edge.destination instanceof AudioParam ?
                audioParamState.get(edge.destination).owner : edge.destination));
    const audioNodeReaches = (source, target) => {
        if (source === target) return true;
        const seen = new Set([source]);
        const pending = audioNodeSuccessors(source);
        while (pending.length) {
            const node = pending.pop();
            if (node === target) return true;
            if (seen.has(node)) continue;
            seen.add(node);
            pending.push(...audioNodeSuccessors(node));
        }
        return false;
    };
    const finishAudioSource = node => {
        const state = audioScheduledState.get(node);
        if (!state.ended) {
            state.ended = true;
            audioTask(() => node.dispatchEvent(new AudioEvent('ended')));
        }
    };

    class AudioNode extends EventTarget {
        constructor(token, context, inputs, outputs, channels) {
            super();
            if (token !== audioNodeToken) throw new TypeError('Illegal constructor');
            if (!(context instanceof BaseAudioContext))
                throw new TypeError('AudioNode requires a BaseAudioContext');
            const contextState = audioContextState.get(context);
            if (contextState.nodes.size >= MAX_AUDIO_NODES)
                throw new DOMException('Audio graph node limit reached', 'NotSupportedError');
            if (contextState.renderStarted &&
                !audioWorkWithinLimit(contextState, contextState.nodes.size + 1))
                throw new DOMException('Audio graph exceeds the rendering work limit',
                    'NotSupportedError');
            Object.defineProperties(this, {
                context: { enumerable: true, value: context },
                numberOfInputs: { enumerable: true, value: inputs },
                numberOfOutputs: { enumerable: true, value: outputs }
            });
            audioNodeState.set(this, {
                ...(channels ?? audioChannelSettings(context,
                    readAudioChannelOptions(context, {}))),
                inputs: Array.from({ length: inputs }, () => new Set()),
                outputs: Array.from({ length: outputs }, () => new Set())
            });
            contextState.nodes.add(this);
            contextState.graphRevision++;
        }
        get channelCount() { return ownedAudioNodeState(this).channelCount; }
        set channelCount(value) { setAudioChannelSetting(this, 'channelCount', value); }
        get channelCountMode() { return ownedAudioNodeState(this).channelCountMode; }
        set channelCountMode(value) { setAudioChannelSetting(this, 'channelCountMode', value); }
        get channelInterpretation() { return ownedAudioNodeState(this).channelInterpretation; }
        set channelInterpretation(value) {
            setAudioChannelSetting(this, 'channelInterpretation', value);
        }
        connect(destination, output = 0, input = 0) {
            return connectAudioNode(this, destination, output, input);
        }
        disconnect(destinationOrOutput, output, input) {
            return disconnectAudioNode(this, Array.from(arguments));
        }
    }

    class AudioDestinationNode extends AudioNode {
        constructor(token, context) {
            if (token !== audioNodeToken) throw new TypeError('Illegal constructor');
            super(audioNodeToken, context, 1, 0, audioChannelSettings(context,
                readAudioChannelOptions(context, {}), 'destination',
                audioContextState.get(context).channels));
            Object.defineProperty(this, 'maxChannelCount', { enumerable: true,
                value: audioContextState.get(context).channels });
            audioNodeState.get(this).render = (frame, frames, cache) =>
                mixAudioInputs(this.context, this, frame, frames, cache);
        }
    }

    class GainNode extends AudioNode {
        constructor(context, options = {}) {
            const channels = readAudioChannelOptions(context, options);
            options = channels.options;
            const gain = audioOption(options, 'gain', 1, value => finiteFloat(value, 'gain'));
            super(audioNodeToken, context, 1, 1, audioChannelSettings(context, channels));
            Object.defineProperty(this, 'gain', { enumerable: true,
                value: new AudioParam(audioParamToken, context,
                    gain,
                    -3.4028234663852886e38, 3.4028234663852886e38) });
            audioParamState.get(this.gain).owner = this;
            audioNodeState.get(this).render = (frame, frames, cache) => {
            const samples = mixAudioInputs(this.context, this, frame, frames, cache);
            for (let i = 0; i < frames; ++i) {
                const value = audioParamValueForFrame(this.gain, frame, i,
                    this.context.sampleRate, cache);
                for (const channel of samples) channel[i] *= value;
            }
            return samples;
            };
        }
    }

    class AudioScheduledSourceNode extends AudioNode {
        constructor(token, context, channels) {
            if (token !== audioNodeToken) throw new TypeError('Illegal constructor');
            super(audioNodeToken, context, 0, 1, channels);
            audioScheduledState.set(this, { start: null, stop: Infinity,
                ended: false, onended: null });
        }
        get onended() { return audioScheduledState.get(this).onended; }
        set onended(callback) {
            const state = audioScheduledState.get(this);
            if (state.onended) this.removeEventListener('ended', state.onended);
            state.onended = typeof callback === 'function' ? callback : null;
            if (state.onended) this.addEventListener('ended', state.onended);
        }
        start(when = 0) {
            const state = audioScheduledState.get(this);
            if (state.start !== null)
                throw new DOMException('Source has already started', 'InvalidStateError');
            state.start = nonnegative(when, 'when');
        }
        stop(when = 0) {
            const state = audioScheduledState.get(this);
            if (state.start === null)
                throw new DOMException('Source has not started', 'InvalidStateError');
            state.stop = nonnegative(when, 'when');
        }
    }

    class OscillatorNode extends AudioScheduledSourceNode {
        constructor(context, options = {}) {
            const channels = readAudioChannelOptions(context, options);
            options = channels.options;
            const detune = audioOption(options, 'detune', 0,
                value => finiteFloat(value, 'detune'));
            const frequency = audioOption(options, 'frequency', 440,
                value => finiteFloat(value, 'frequency'));
            const wave = options.periodicWave;
            if (wave !== undefined && !(wave instanceof PeriodicWave))
                throw new TypeError('periodicWave must be a PeriodicWave');
            const type = audioOption(options, 'type', 'sine', value => `${value}`);
            if (!['sine', 'square', 'sawtooth', 'triangle', 'custom'].includes(type))
                throw new TypeError('Unsupported oscillator type');
            if (type === 'custom' && wave === undefined)
                throw new AudioDOMException('Custom oscillator needs a PeriodicWave',
                    'InvalidStateError');
            if (wave !== undefined && periodicWaveState.get(wave).context !== context)
                throw new AudioDOMException('PeriodicWave belongs to a different context',
                    'InvalidAccessError');
            super(audioNodeToken, context, audioChannelSettings(context, channels));
            audioOscillatorState.set(this, { type: wave ? 'custom' : 'sine',
                phase: 0, wave: wave ?? null });
            if (!wave) this.type = type;
            Object.defineProperties(this, {
                frequency: { enumerable: true, value: new AudioParam(audioParamToken, context,
                    frequency,
                    -context.sampleRate / 2, context.sampleRate / 2) },
                detune: { enumerable: true, value: new AudioParam(audioParamToken, context,
                    detune,
                    -153600, 153600) }
            });
            audioParamState.get(this.frequency).owner = this;
            audioParamState.get(this.detune).owner = this;
            audioNodeState.get(this).render = (frame, frames, cache) =>
                renderOscillator(this, frame, frames, cache);
        }
        get type() { return audioOscillatorState.get(this).type; }
        set type(value) {
            const state = audioOscillatorState.get(this);
            if (!state) throw new TypeError('Illegal OscillatorNode receiver');
            value = audioEnumAttribute(value, ['sine', 'square', 'sawtooth', 'triangle', 'custom']);
            if (value === null) return;
            if (value === 'custom')
                throw new AudioDOMException('Use setPeriodicWave for a custom waveform',
                    'InvalidStateError');
            state.type = value;
        }
        setPeriodicWave(wave) {
            if (!(wave instanceof PeriodicWave))
                throw new TypeError('setPeriodicWave requires a PeriodicWave');
            if (periodicWaveState.get(wave).context !== this.context)
                throw new AudioDOMException('PeriodicWave belongs to a different context',
                    'InvalidAccessError');
            const state = audioOscillatorState.get(this);
            state.wave = wave;
            state.type = 'custom';
        }
    }
    const renderOscillator = (node, frame, frames, cache) => {
        const output = silence(1, frames)[0];
        const source = audioScheduledState.get(node);
        const oscillator = audioOscillatorState.get(node);
        if (source.start === null) return [output];
        for (let i = 0; i < frames; ++i) {
            const time = (frame + i) / node.context.sampleRate;
            if (time < source.start) continue;
            if (time >= source.stop) { finishAudioSource(node); break; }
            const phase = oscillator.phase;
            const frequency = audioParamValueForFrame(node.frequency, frame, i,
                node.context.sampleRate, cache) * Math.pow(2,
                audioParamValueForFrame(node.detune, frame, i,
                    node.context.sampleRate, cache) / 1200);
            if (oscillator.type === 'custom') output[i] = periodicWaveSample(
                oscillator.wave, phase, frequency, node.context.sampleRate);
            else if (oscillator.type === 'sine') output[i] = Math.sin(2 * Math.PI * phase);
            else if (oscillator.type === 'square') output[i] = phase < 0.5 ? 1 : -1;
            else if (oscillator.type === 'sawtooth') output[i] = 2 * phase - 1;
            else output[i] = 1 - 4 * Math.abs(phase - 0.5);
            oscillator.phase = ((phase + frequency / node.context.sampleRate) % 1 + 1) % 1;
        }
        return [output];
    };
