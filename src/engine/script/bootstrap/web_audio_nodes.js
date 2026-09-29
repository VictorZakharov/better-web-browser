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
        constructor(token, context, inputs, outputs) {
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
                inputs: Array.from({ length: inputs }, () => new Set()),
                outputs: Array.from({ length: outputs }, () => new Set())
            });
            contextState.nodes.add(this);
        }
        connect(destination, output = 0, input = 0) {
            const param = destination instanceof AudioParam ?
                audioParamState.get(destination) : null;
            if (!(destination instanceof AudioNode) && !param)
                throw new TypeError('Destination must be an AudioNode or AudioParam');
            if ((param ? param.context : destination.context) !== this.context)
                throw new DOMException('Cannot connect different audio contexts', 'InvalidAccessError');
            output = Number(output) >>> 0;
            input = Number(input) >>> 0;
            if (output >= this.numberOfOutputs || (!param &&
                input >= destination.numberOfInputs))
                throw new DOMException('Input or output index is out of range', 'IndexSizeError');
            if (audioNodeReaches(param ? param.owner : destination, this))
                throw new DOMException('A cycle needs a delay node', 'NotSupportedError');
            const state = audioNodeState.get(this);
            if (![...state.outputs[output]].some(edge =>
                edge.destination === destination && (param || edge.input === input))) {
                if (audioContextState.get(this.context).connections >= MAX_AUDIO_CONNECTIONS)
                    throw new DOMException('Audio graph connection limit reached', 'NotSupportedError');
                const edge = { source: this, destination, output, input };
                state.outputs[output].add(edge);
                if (param) param.inputs.add(edge);
                else audioNodeState.get(destination).inputs[input].add(edge);
                audioContextState.get(this.context).connections++;
            }
            if (!param) return destination;
        }
        disconnect(destinationOrOutput, output, input) {
            const destination = destinationOrOutput instanceof AudioNode ||
                destinationOrOutput instanceof AudioParam ? destinationOrOutput : undefined;
            const param = destination instanceof AudioParam ?
                audioParamState.get(destination) : null;
            if (destinationOrOutput !== undefined && destination === undefined &&
                typeof destinationOrOutput !== 'number')
                throw new TypeError('Destination must be an AudioNode, AudioParam, or output index');
            const state = audioNodeState.get(this);
            const outputIndex = destination === undefined ? destinationOrOutput : output;
            const selectedOutput = outputIndex === undefined ? undefined :
                Number(outputIndex) >>> 0;
            const selectedInput = input === undefined || param ? undefined :
                Number(input) >>> 0;
            if (selectedOutput !== undefined && selectedOutput >= state.outputs.length)
                throw new DOMException('Output index is out of range', 'IndexSizeError');
            if (destination && !param && selectedInput !== undefined &&
                selectedInput >= destination.numberOfInputs)
                throw new DOMException('Input index is out of range', 'IndexSizeError');
            let removed = 0;
            for (let port = 0; port < state.outputs.length; ++port) {
                if (selectedOutput !== undefined && port !== selectedOutput) continue;
                for (const edge of [...state.outputs[port]]) {
                    if (destination && edge.destination !== destination) continue;
                    if (selectedInput !== undefined && edge.input !== selectedInput) continue;
                    state.outputs[port].delete(edge);
                    if (edge.destination instanceof AudioParam)
                        audioParamState.get(edge.destination).inputs.delete(edge);
                    else audioNodeState.get(edge.destination).inputs[edge.input].delete(edge);
                    audioContextState.get(this.context).connections--;
                    removed++;
                }
            }
            if (destination && !removed)
                throw new DOMException('No connection to destination', 'InvalidAccessError');
        }
    }

    class AudioDestinationNode extends AudioNode {
        constructor(token, context) {
            if (token !== audioNodeToken) throw new TypeError('Illegal constructor');
            super(audioNodeToken, context, 1, 0);
            Object.defineProperty(this, 'maxChannelCount', { enumerable: true,
                value: audioContextState.get(context).channels });
            audioNodeState.get(this).render = (frame, frames, cache) =>
                mixAudioInputs(this.context, this, frame, frames, cache,
                    this.maxChannelCount);
        }
    }

    class GainNode extends AudioNode {
        constructor(context, options = {}) {
            const gain = options.gain === undefined ? 1 : finiteFloat(options.gain, 'gain');
            super(audioNodeToken, context, 1, 1);
            Object.defineProperty(this, 'gain', { enumerable: true,
                value: new AudioParam(audioParamToken, context,
                    gain,
                    -3.4028234663852886e38, 3.4028234663852886e38) });
            audioParamState.get(this.gain).owner = this;
            audioNodeState.get(this).render = (frame, frames, cache) => {
            const samples = mixAudioInputs(this.context, this, frame, frames, cache,
                audioContextState.get(this.context).channels);
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
        constructor(token, context) {
            if (token !== audioNodeToken) throw new TypeError('Illegal constructor');
            super(audioNodeToken, context, 0, 1);
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
            const type = options.type === undefined ? 'sine' : options.type;
            if (!['sine', 'square', 'sawtooth', 'triangle', 'custom'].includes(type))
                throw new TypeError('Unsupported oscillator type');
            const wave = options.periodicWave;
            if (type === 'custom' && wave === undefined)
                throw new AudioDOMException('Custom oscillator needs a PeriodicWave',
                    'InvalidStateError');
            if (wave !== undefined && !(wave instanceof PeriodicWave))
                throw new TypeError('periodicWave must be a PeriodicWave');
            if (wave !== undefined && periodicWaveState.get(wave).context !== context)
                throw new AudioDOMException('PeriodicWave belongs to a different context',
                    'InvalidAccessError');
            const frequency = options.frequency === undefined ? 440 :
                finiteFloat(options.frequency, 'frequency');
            const detune = options.detune === undefined ? 0 :
                finiteFloat(options.detune, 'detune');
            super(audioNodeToken, context);
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
            if (value === 'custom')
                throw new AudioDOMException('Use setPeriodicWave for a custom waveform',
                    'InvalidStateError');
            if (!['sine', 'square', 'sawtooth', 'triangle'].includes(value))
                throw new TypeError('Unsupported oscillator type');
            audioOscillatorState.get(this).type = value;
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
