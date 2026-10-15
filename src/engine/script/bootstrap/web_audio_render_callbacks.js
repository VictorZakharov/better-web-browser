    let audioThreadArrays = [];
    const queueAudioThreadProcessing = (node, playbackFrame) => {
        const state = scriptProcessorState(node);
        if (state.pending) return;
        state.pending = true;
        const inputs = state.input.map(channel => {
            const index = audioThreadArrays.length;
            audioThreadArrays.push(new Uint8Array(new Float32Array(channel).buffer));
            return index;
        });
        // Legacy author processing needs a future block. The DSP thread keeps
        // advancing if its main-thread callback misses this deadline (§1.29).
        audioThreadEvents.push({ id: audioRenderIds.get(node), generation: state.generation,
            playbackFrame: playbackFrame + state.bufferSize, inputs });
    };
    const deliverAudioRenderCallbacks = (context, events, arrays, offset) => {
        for (const event of events) {
            const reference = audioRenderObjects.get(event.id);
            const node = reference && derefAudioSource(reference);
            if (!node || !(node instanceof ScriptProcessorNode)) continue;
            const state = scriptProcessorState(node);
            const input = processingAudioBuffer(state.inputChannels, state.bufferSize, context.sampleRate);
            const output = processingAudioBuffer(state.outputChannels, state.bufferSize, context.sampleRate);
            audioBufferData.get(input).forEach((channel, index) => {
                const bytes = arrays[offset + event.inputs[index]];
                channel.set(new Float32Array(bytes.buffer, bytes.byteOffset, bytes.byteLength / 4));
            });
            audioTask(() => {
                if (context.state === 'closed' || !scriptProcessorConnected(node)) return;
                const processing = new AudioProcessingEvent('audioprocess', { inputBuffer: input,
                    outputBuffer: output, playbackTime: event.playbackFrame / context.sampleRate });
                dispatchAudioProcessing.call(node, trustAudioEvent(processing));
                // Copy before microtasks can mutate the event's output buffer.
                const channels = audioBufferData.get(output).map(channel =>
                    new Uint8Array((channel.length === state.bufferSize ? new Float32Array(channel) :
                        new Float32Array(state.bufferSize)).buffer));
                audioHost('audioRenderCallback', event.id, event.generation,
                    event.playbackFrame, channels);
            });
        }
    };
    if (audioRenderThread) Object.defineProperty(globalThis, '__audioThreadCallback', {
        configurable: true, value: (id, generation, playbackFrame, channels) => {
            const reference = audioThreadNodes.get(id)?.node;
            const node = reference && derefAudioSource(reference);
            if (!node) return;
            const state = scriptProcessorState(node);
            if (state.generation !== generation) return;
            state.pending = false;
            const frame = audioContextState.get(node.context).live.renderFrame;
            if (frame > playbackFrame || !scriptProcessorConnected(node)) return;
            state.threadOutput = { frame: playbackFrame, channels: channels.map(bytes =>
                new Float32Array(bytes.buffer, bytes.byteOffset, bytes.byteLength / 4)) };
        }
    });
