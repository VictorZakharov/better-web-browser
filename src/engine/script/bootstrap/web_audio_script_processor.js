    // Legacy Web Audio §1.29/§1.12: actual asynchronous author processing, not
    // an AudioWorklet substitute. Offline and live contexts share PCM blocks.
    const audioScriptProcessorToken = {};
    const audioScriptProcessorState = new WeakMap();
    const MAX_AUDIO_SCRIPT_BYTES = 16 * 1024 * 1024;
    const dispatchAudioProcessing = EventTarget.prototype.dispatchEvent;
    const scriptProcessorState = node => {
        const state = audioScriptProcessorState.get(node);
        if (!state) throw new TypeError('Illegal ScriptProcessorNode receiver');
        return state;
    };
    const scriptProcessorConnected = node => {
        const state = audioNodeState.get(node);
        return state.inputs.some(port => port.size) || state.outputs.some(port => port.size);
    };
    const createAudioScriptProcessor = (context, bufferSize, inputChannels, outputChannels) => {
        if (!audioContextState.has(context)) throw new TypeError('Illegal BaseAudioContext receiver');
        bufferSize = audioChannelUnsigned(bufferSize);
        inputChannels = audioChannelUnsigned(inputChannels);
        outputChannels = audioChannelUnsigned(outputChannels);
        if (bufferSize === 0) bufferSize = 2048;
        if (![256, 512, 1024, 2048, 4096, 8192, 16384].includes(bufferSize))
            throw new AudioDOMException('ScriptProcessor bufferSize must be zero or a power of two from 256 to 16384',
                'IndexSizeError');
        if (inputChannels > 32 || outputChannels > 32 || !(inputChannels || outputChannels))
            throw new AudioDOMException('ScriptProcessor needs 0–32 input/output channels, with a nonzero side',
                'IndexSizeError');
        return new ScriptProcessorNode(audioScriptProcessorToken, context, bufferSize,
            inputChannels, outputChannels);
    };
    class ScriptProcessorNode extends AudioNode {
        constructor(token, context, bufferSize, inputChannels, outputChannels) {
            if (token !== audioScriptProcessorToken) throw new TypeError('Illegal constructor');
            const contextState = audioContextState.get(context);
            // Worst-case node-owned storage: accumulation + event input copy,
            // event output + acquired playback copy. Author-retained event
            // buffers are ordinary author-owned allocations after dispatch.
            const bytes = 2 * bufferSize * (inputChannels + outputChannels) * 4;
            if (bytes > MAX_AUDIO_SCRIPT_BYTES - contextState.scriptProcessorBytes)
                throw new AudioDOMException('ScriptProcessor buffers exceed the context memory limit',
                    'NotSupportedError');
            const channels = { kind: 'script-processor', fixedCount: inputChannels,
                channelCount: inputChannels, channelCountMode: 'explicit',
                channelInterpretation: 'speakers' };
            super(audioNodeToken, context, 1, 1, channels);
            audioScriptProcessorState.set(this, { bufferSize, inputChannels, outputChannels,
                input: silence(inputChannels, bufferSize), output: silence(outputChannels, bufferSize),
                position: 0, generation: 0, pending: false, onprocess: null });
            contextState.scriptProcessorBytes += bytes;
            contextState.scriptProcessors.add(this);
            audioNodeState.get(this).render = (frame, frames, cache) =>
                renderAudioScriptProcessor(this, frame, frames, cache);
        }
        get bufferSize() { return scriptProcessorState(this).bufferSize; }
        get onaudioprocess() { return scriptProcessorState(this).onprocess; }
        set onaudioprocess(callback) {
            const state = scriptProcessorState(this);
            if (state.onprocess) this.removeEventListener('audioprocess', state.onprocess);
            state.onprocess = typeof callback === 'function' ? callback : null;
            if (state.onprocess) this.addEventListener('audioprocess', state.onprocess);
        }
    }
    const resetAudioScriptProcessor = state => {
        state.position = 0;
        state.generation++;
        state.pending = false;
        for (const channel of state.input) channel.fill(0);
        for (const channel of state.output) channel.fill(0);
    };
    const queueAudioProcessing = (node, playbackFrame) => {
        const state = scriptProcessorState(node);
        // A delayed callback cannot build an unbounded author-event queue or
        // retroactively fill audio that has already been submitted to a device.
        if (state.pending) return;
        state.pending = true;
        const generation = state.generation;
        const rate = node.context.sampleRate;
        const input = processingAudioBuffer(state.inputChannels, state.bufferSize, rate);
        const output = processingAudioBuffer(state.outputChannels, state.bufferSize, rate);
        audioBufferData.get(input).forEach((channel, index) => channel.set(state.input[index]));
        audioTask(() => {
            if (state.generation !== generation) return;
            state.pending = false;
            if (node.context.state === 'closed' || !scriptProcessorConnected(node)) return;
            const event = new AudioProcessingEvent('audioprocess', { inputBuffer: input,
                outputBuffer: output, playbackTime: playbackFrame / rate });
            dispatchAudioProcessing.call(node, trustAudioEvent(event));
            // Acquire before the callback's microtasks. Later edits to an
            // event's channel arrays cannot change an already produced block.
            const context = audioContextState.get(node.context);
            const renderedFrame = context.live?.renderFrame ?? context.time * rate;
            if (state.generation === generation && scriptProcessorConnected(node) &&
                renderedFrame <= playbackFrame)
                state.output = audioBufferData.get(output).map(channel =>
                    channel.length === state.bufferSize ? new Float32Array(channel) :
                        new Float32Array(state.bufferSize));
        });
    };
    const renderAudioScriptProcessor = (node, frame, frames, cache) => {
        const state = scriptProcessorState(node);
        if (!scriptProcessorConnected(node)) {
            resetAudioScriptProcessor(state);
            return silence(Math.max(1, state.outputChannels), frames);
        }
        const input = state.inputChannels ?
            mixAudioInputs(node.context, node, frame, frames, cache) : [];
        const result = silence(Math.max(1, state.outputChannels), frames);
        for (let channel = 0; channel < state.outputChannels; ++channel)
            result[channel].set(state.output[channel].subarray(state.position, state.position + frames));
        for (let channel = 0; channel < state.inputChannels; ++channel)
            state.input[channel].set(input[channel], state.position);
        state.position += frames;
        if (state.position === state.bufferSize) {
            state.position = 0;
            queueAudioProcessing(node, frame + frames);
            // If the queued task misses its deadline, this next block is
            // silence rather than a replay of the previous author output.
            for (const channel of state.output) channel.fill(0);
        }
        return result;
    };
    const audioScriptTaskQuanta = (context, maximum) => {
        for (const node of audioContextState.get(context).scriptProcessors) {
            if (!scriptProcessorConnected(node)) continue;
            const state = scriptProcessorState(node);
            maximum = Math.min(maximum,
                Math.ceil((state.bufferSize - state.position) / AUDIO_QUANTUM));
        }
        return maximum;
    };
    const retireAudioScriptProcessors = context => {
        for (const node of audioContextState.get(context).scriptProcessors)
            resetAudioScriptProcessor(scriptProcessorState(node));
    };
    for (const name of ['bufferSize', 'onaudioprocess'])
        Object.defineProperty(ScriptProcessorNode.prototype, name, {
            ...Object.getOwnPropertyDescriptor(ScriptProcessorNode.prototype, name), enumerable: true
        });
