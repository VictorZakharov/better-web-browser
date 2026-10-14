    let audioThreadContext = null;
    const audioThreadNodes = new Map();
    const audioThreadAssets = new Map();
    const audioThreadWaveIds = new Set();
    let audioThreadEdges = '';
    let audioThreadEvents = [];
    const audioThreadNode = id => {
        const reference = audioThreadNodes.get(id)?.node;
        return reference && derefAudioSource(reference);
    };
    const audioThreadAsset = id => id === null ? null : audioThreadAssets.get(id);
    const audioThreadSetParameters = (node, parameters) => {
        for (const [name, value] of Object.entries(parameters)) {
            const parameter = audioParamState.get(node[name]);
            parameter.intrinsic = value.intrinsic;
            parameter.rate = value.rate;
            parameter.events = value.events.map(event => ({ ...event,
                values: event.values === undefined ? undefined : audioThreadAsset(event.values) }));
            parameter.timelineDirty = true;
        }
    };
    const audioThreadCreate = control => {
        const context = audioThreadContext;
        const type = audioRenderKinds[control.kind];
        const options = { ...control.channels, ...control.extra };
        if (type === AudioDestinationNode) return context.destination;
        if (type === ScriptProcessorNode)
            return createAudioScriptProcessor(context, options.bufferSize,
                options.inputChannels, options.outputChannels);
        if (type === MediaStreamAudioSourceNode) {
            // Track ownership and capture permission stay on the control thread.
            const node = new AudioNode(audioNodeToken, context, 0, 1);
            Object.setPrototypeOf(node, MediaStreamAudioSourceNode.prototype);
            audioCaptureState.set(node, { track: options.track, packets: [], offset: 0, channels: 1 });
            audioNodeState.get(node).render = (_frame, frames) => renderCaptureAudio(node, frames);
            return node;
        }
        // These private acquired resources are installed below, without running
        // author getters or repeating acquisition/normalization on a mutable buffer.
        delete options.type;
        delete options.buffer;
        delete options.curve;
        return new type(context, options);
    };
    const audioThreadSetExtra = (node, control, previous) => {
        const extra = control.extra;
        if (node instanceof AudioBufferSourceNode) {
            const state = audioBufferSourceState.get(node);
            for (const key of ['loop', 'loopStart', 'loopEnd', 'offset']) state[key] = extra[key];
            state.duration = extra.duration ?? Infinity;
            if (!previous || previous.acquired !== extra.acquired) {
                releaseBufferSourceContent(node);
                state.acquired = audioThreadAsset(extra.acquired);
                if (state.acquired) {
                    audioContextState.get(node.context).sourceSnapshotBytes += state.acquired.bytes;
                    state.position = Math.min(state.offset,
                        state.acquired.length / state.acquired.sampleRate) * state.acquired.sampleRate;
                }
            }
        } else if (node instanceof OscillatorNode) {
            const state = audioOscillatorState.get(node);
            state.type = extra.type;
            if (extra.wave !== previous?.wave && extra.wave) {
                const acquired = audioThreadAsset(extra.wave);
                if (!acquired.wave) {
                    acquired.wave = Object.create(PeriodicWave.prototype);
                    periodicWaveState.set(acquired.wave, { context: node.context,
                        coefficients: acquired.coefficients, size: acquired.size,
                        maximumHarmonic: acquired.maximumHarmonic, scale: acquired.scale,
                        directHarmonics: 0, tables: new Map([[acquired.maximumHarmonic, acquired.table]]) });
                    if (!audioThreadWaveIds.has(extra.wave)) {
                        periodicWaveBytes.set(node.context, (periodicWaveBytes.get(node.context) ?? 0) +
                            acquired.table.byteLength + acquired.coefficients.real.byteLength +
                            acquired.coefficients.imag.byteLength);
                        audioThreadWaveIds.add(extra.wave);
                    }
                }
                state.wave = acquired.wave;
            }
        } else if (node instanceof BiquadFilterNode) audioBiquadState.get(node).type = extra.type;
        else if (node instanceof PannerNode) Object.assign(spatialPannerState.get(node), extra);
        else if (node instanceof WaveShaperNode) {
            const state = audioWaveShaperState.get(node);
            if (state.oversample !== extra.oversample) state.oversampleDsp = null;
            state.oversample = extra.oversample;
            state.samples = audioThreadAsset(extra.samples);
        } else if (node instanceof ConvolverNode) {
            const state = audioConvolverState.get(node);
            if (extra.bank !== previous?.bank) {
                state.bank = audioThreadAsset(extra.bank);
                state.buffer = extra.buffer;
                state.channelDeadlines = null;
            }
        } else if (node instanceof AnalyserNode) {
            const state = analyserState.get(node);
            if (state.size !== extra.size) {
                state.frequency = new Float32Array(extra.size / 2);
                state.frequencyFrame = -2;
            }
            Object.assign(state, extra);
        } else if (audioCaptureState.has(node)) {
            const state = audioCaptureState.get(node);
            if (state.generation !== extra.generation) clearCaptureAudio(state);
            state.generation = extra.generation;
            state.track = extra.track;
            for (const id of extra.packets) {
                const packet = audioThreadAsset(id);
                if (state.lastSequence !== null && packet.sequence <= state.lastSequence) continue;
                if (state.packets.length === MAX_CAPTURE_AUDIO_PACKETS) {
                    state.packets.shift(); state.offset = 0;
                }
                state.packets.push(packet);
                state.channels = packet.channels;
                state.lastSequence = packet.sequence;
            }
        }
    };
    const audioThreadApplyGraph = serialized => {
        const graph = audioRenderParse(serialized);
        const context = audioThreadContext;
        for (const control of graph.nodes) {
            let entry = audioThreadNodes.get(control.id);
            if (!entry) {
                const node = audioThreadCreate(control);
                entry = { node: new AudioSourceWeakRef(node), retained: node, extra: null };
                audioThreadNodes.set(control.id, entry);
                audioRenderIds.set(node, control.id);
            }
            const node = derefAudioSource(entry.node);
            if (!node) continue;
            Object.assign(audioNodeState.get(node), control.channels);
            audioThreadSetParameters(node, control.parameters);
            audioThreadSetExtra(node, control, entry.extra);
            entry.extra = control.extra;
            if (control.schedule) {
                const schedule = audioScheduledState.get(node);
                schedule.start = control.schedule.start;
                schedule.stop = control.schedule.stop ?? Infinity;
            }
        }
        audioThreadSetParameters(context.listener, graph.listener.parameters);
        const edges = audioRenderStringify(graph.edges);
        if (edges !== audioThreadEdges) {
            for (const entry of audioThreadNodes.values()) {
                const node = derefAudioSource(entry.node);
                if (node) node.disconnect();
            }
            for (const [sourceId, destinationId, parameter, output, input] of graph.edges) {
                const source = audioThreadNode(sourceId);
                const destination = destinationId === graph.listener.id ? context.listener :
                    audioThreadNode(destinationId);
                if (!source || !destination) continue;
                if (parameter) source.connect(destination[parameter], output);
                else source.connect(destination, output, input);
            }
            audioThreadEdges = edges;
        }
        const state = audioContextState.get(context);
        for (const entry of audioThreadNodes.values())
            if (state.nodes.has(derefAudioSource(entry.node))) entry.retired = false;
        state.graphRevision++;
        state.graphPlan = null;
        for (const id of audioThreadAssets.keys())
            if (!graph.assets.includes(id)) audioThreadAssets.delete(id);
        const alive = new Set(graph.nodes.map(node => node.id));
        for (const id of audioThreadNodes.keys()) if (!alive.has(id)) audioThreadNodes.delete(id);
    };
    if (audioRenderThread) Object.defineProperties(globalThis, {
        __audioThreadInit: { configurable: true, value: rate => {
            audioThreadContext = new AudioContext({ sampleRate: rate });
            const state = audioContextState.get(audioThreadContext);
            state.renderStarted = true;
            state.state = 'running';
            state.live.phase = 'active';
        } },
        __audioThreadAsset: { configurable: true, value: (id, description, arrays) => {
            const types = { Float32Array, Float64Array, Int16Array, Uint8Array, Uint32Array };
            const decode = value => {
                if (value && value.array !== undefined)
                    return new types[value.type](arrays[value.array].buffer);
                if (Array.isArray(value)) return value.map(decode);
                if (value && typeof value === 'object')
                    return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, decode(v)]));
                return value;
            };
            audioThreadAssets.set(id, decode(audioRenderParse(description)));
        } },
        __audioThreadGraph: { configurable: true, value: audioThreadApplyGraph },
        __audioThreadRender: { configurable: true, value: () => renderLiveChunk(audioThreadContext).bytes },
        __audioThreadFrame: { configurable: true, value: () => audioContextState.get(audioThreadContext).live.renderFrame },
        __audioThreadEvents: { configurable: true, value: () => {
            const events = audioThreadEvents;
            audioThreadEvents = [];
            const context = audioContextState.get(audioThreadContext);
            const retired = [];
            for (const [id, entry] of audioThreadNodes) {
                const node = derefAudioSource(entry.node);
                if (node && !context.nodes.has(node) && !entry.retired) {
                    entry.retired = true;
                    retired.push([id, !!audioScheduledState.get(node)?.ended]);
                }
            }
            const arrays = audioThreadArrays;
            audioThreadArrays = [];
            return [audioRenderStringify({ events, retired }), ...arrays];
        } }
    });
