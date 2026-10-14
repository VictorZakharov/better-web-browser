    // The control realm publishes values, never executable page objects. The
    // rendering realm keeps its own nodes, sample positions and filter history.
    // https://webaudio.github.io/web-audio-api/#rendering-loop
    const audioRenderThread = globalThis.__audioRenderingThread === true;
    delete globalThis.__audioRenderingThread;
    const audioRenderStringify = JSON.stringify;
    const audioRenderParse = JSON.parse;
    const audioRenderDescriptors = Object.getOwnPropertyDescriptors;
    const audioRenderKinds = [AudioDestinationNode, AudioBufferSourceNode, OscillatorNode,
        ConstantSourceNode, GainNode, StereoPannerNode, PannerNode, DelayNode,
        ChannelSplitterNode, ChannelMergerNode, IIRFilterNode, BiquadFilterNode,
        DynamicsCompressorNode, ConvolverNode, WaveShaperNode, AnalyserNode,
        ScriptProcessorNode, MediaStreamAudioSourceNode];
    const audioRenderIds = new WeakMap();
    const audioRenderObjects = new Map();
    const audioRenderAssets = new WeakMap();
    let audioRenderUsedAssets = new Set();
    let audioRenderKnownAssets = new Set();
    let nextAudioRenderId = 1;
    let nextAudioAssetId = 1;
    const audioRenderId = object => {
        let id = audioRenderIds.get(object);
        if (!id) {
            id = nextAudioRenderId++;
            audioRenderIds.set(object, id);
        }
        audioRenderObjects.set(id, new AudioSourceWeakRef(object));
        return id;
    };
    const audioRenderAsset = (object, data = object) => {
        if (!object) return null;
        let id = audioRenderAssets.get(object);
        if (!id || (!audioRenderKnownAssets.has(id) && !audioRenderUsedAssets.has(id))) {
            id ??= nextAudioAssetId++;
            const arrays = [];
            const encode = value => {
                if (ArrayBuffer.isView(value)) {
                    const index = arrays.length;
                    arrays.push(new Uint8Array(value.buffer, value.byteOffset, value.byteLength));
                    return { array: index, type: value.constructor.name };
                }
                if (Array.isArray(value)) return Array.from(value, encode);
                if (value && typeof value === 'object')
                    return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, encode(v)]));
                return value ?? null;
            };
            const description = audioRenderStringify(encode(data));
            if (!audioHost('audioRenderAsset', id, description, arrays))
                throw new AudioDOMException('Audio rendering transfer budget exceeded', 'NotSupportedError');
            audioRenderAssets.set(object, id);
        }
        audioRenderUsedAssets.add(id);
        return id;
    };
    const audioRenderParameters = node => Object.fromEntries(
        Object.entries(audioRenderDescriptors(node)).filter(([, property]) =>
            audioParamState.has(property.value)).map(([name, property]) => {
            const param = audioParamState.get(property.value);
            return [name, { intrinsic: param.intrinsic, rate: param.rate,
                events: param.events.map(event => ({ ...event,
                    values: event.values ? audioRenderAsset(event.values) : undefined })) }];
        }));
    const audioRenderNode = node => {
        const state = audioNodeState.get(node);
        const kind = audioRenderKinds.findIndex(type => node instanceof type);
        const control = { id: audioRenderId(node), kind, parameters: audioRenderParameters(node),
            channels: { channelCount: state.channelCount, channelCountMode: state.channelCountMode,
                channelInterpretation: state.channelInterpretation }, extra: {} };
        const extra = control.extra;
        const scheduled = audioScheduledState.get(node);
        if (scheduled) control.schedule = { start: scheduled.start,
            stop: Number.isFinite(scheduled.stop) ? scheduled.stop : null };
        if (node instanceof AudioBufferSourceNode) {
            const buffer = audioBufferSourceState.get(node);
            Object.assign(extra, { loop: buffer.loop, loopStart: buffer.loopStart,
                loopEnd: buffer.loopEnd, offset: buffer.offset,
                duration: Number.isFinite(buffer.duration) ? buffer.duration : null,
                acquired: audioRenderAsset(buffer.acquired) });
        } else if (node instanceof OscillatorNode) {
            const oscillator = audioOscillatorState.get(node);
            extra.type = oscillator.type;
            const wave = oscillator.wave && periodicWaveState.get(oscillator.wave);
            extra.wave = wave ? audioRenderAsset(oscillator.wave, {
                coefficients: wave.coefficients, scale: wave.scale, size: wave.size,
                maximumHarmonic: wave.maximumHarmonic,
                table: wave.tables.get(wave.maximumHarmonic) }) : null;
        } else if (node instanceof DelayNode) extra.maxDelayTime = audioDelayState.get(node).maxDelayTime;
        else if (node instanceof IIRFilterNode) {
            const filter = audioIirState.get(node);
            extra.feedforward = filter.b; extra.feedback = filter.a;
        } else if (node instanceof BiquadFilterNode) extra.type = audioBiquadState.get(node).type;
        else if (node instanceof ChannelSplitterNode) extra.numberOfOutputs = node.numberOfOutputs;
        else if (node instanceof ChannelMergerNode) extra.numberOfInputs = node.numberOfInputs;
        else if (node instanceof PannerNode) Object.assign(extra, spatialPannerState.get(node));
        else if (node instanceof WaveShaperNode) {
            const shaper = audioWaveShaperState.get(node);
            extra.samples = audioRenderAsset(shaper.samples); extra.oversample = shaper.oversample;
        } else if (node instanceof ConvolverNode) {
            const convolver = audioConvolverState.get(node);
            extra.bank = audioRenderAsset(convolver.bank);
            extra.buffer = convolver.buffer ? audioBufferInfo(convolver.buffer) : null;
        } else if (node instanceof AnalyserNode) {
            const analyser = analyserState.get(node);
            Object.assign(extra, { size: analyser.size, minimum: analyser.minimum,
                maximum: analyser.maximum, smoothing: analyser.smoothing });
        } else if (node instanceof ScriptProcessorNode) {
            const processor = audioScriptProcessorState.get(node);
            Object.assign(extra, { bufferSize: processor.bufferSize,
                inputChannels: processor.inputChannels, outputChannels: processor.outputChannels });
        } else if (node instanceof MediaStreamAudioSourceNode) {
            const capture = audioCaptureState.get(node);
            extra.generation = capture.generation ?? 0;
            extra.track = { readyState: capture.track.readyState, enabled: capture.track.enabled,
                muted: capture.track.muted };
            extra.packets = capture.packets.map(packet => audioRenderAsset(packet));
        }
        return control;
    };
    const audioRenderSnapshot = context => {
        audioRenderUsedAssets = new Set();
        const state = audioContextState.get(context);
        const nodes = [...state.nodes].map(audioRenderNode);
        const edges = [];
        for (const node of state.nodes) for (const port of audioNodeState.get(node).outputs)
            for (const edge of port) {
                const parameter = audioParamState.get(edge.destination);
                const owner = parameter?.owner ?? edge.destination;
                if (!state.nodes.has(owner) && owner !== context.listener) continue;
                const name = parameter ? Object.entries(audioRenderDescriptors(owner))
                    .find(([, property]) => property.value === edge.destination)?.[0] : null;
                edges.push([audioRenderId(node), audioRenderId(owner), name, edge.output, edge.input]);
            }
        return { nodes, edges, assets: [...audioRenderUsedAssets], destination: audioRenderId(context.destination),
            listener: { id: audioRenderId(context.listener), parameters: audioRenderParameters(context.listener) } };
    };
