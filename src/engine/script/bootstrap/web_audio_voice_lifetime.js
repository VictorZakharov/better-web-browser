    // §1.5.3/§3: a completed one-shot releases its playing reference and the
    // processors owned by that voice after their tails drain. Keep logical
    // connections for author-held handles, without making destination inputs
    // retain every source ever played. Capture WeakRef before author scripts.
    const AudioSourceWeakRef = WeakRef;
    const derefAudioSource = Function.prototype.call.bind(WeakRef.prototype.deref);
    const audioCurveFinalizer = new FinalizationRegistry(ledger => {
        const context = derefAudioSource(ledger.context);
        if (context) context.automationCurveBytes -= ledger.bytes;
    });
    const registerAudioCurveFinalizer = Function.prototype.call.bind(
        FinalizationRegistry.prototype.register, audioCurveFinalizer);
    const audioCurveLedger = (parameter, context) => {
        const ledger = { bytes: 0, context: new AudioSourceWeakRef(audioContextState.get(context)) };
        registerAudioCurveFinalizer(parameter, ledger);
        return ledger;
    };
    const activeAudioInput = (context, edge) => {
        const source = edge.source;
        return source && context.nodes.has(source);
    };
    const audioInputEdges = (context, port) => {
        const result = [];
        for (const edge of port) {
            if (!edge.source) removeAudioEdge(context, edge);
            else if (activeAudioInput(context, edge)) result.push(edge);
        }
        return result;
    };
    const deactivateAudioVoiceNode = (context, node) => {
        if (!context.nodes.delete(node)) return;
        context.retiringVoices.delete(node);
        context.graphPlan = null;
        context.graphRevision++;
        for (const port of audioNodeState.get(node).outputs) for (const edge of port) {
            if (edge.counted) { edge.counted = false; context.connections--; }
            const parameter = audioParamState.get(edge.destination);
            if (parameter) parameter.inputs.delete(edge);
            else audioNodeState.get(edge.destination).inputs[edge.input].delete(edge);
        }
    };
    const releaseAudioVoiceNode = (context, node) => {
        deactivateAudioVoiceNode(context, node);
        for (const next of audioNodeSuccessors(node)) considerAudioVoiceNode(context, next);
    };
    const considerAudioVoiceNode = (context, node) => {
        if (!node || node === context.context.destination || !context.nodes.has(node)) return;
        if (audioNodeState.get(node).inputs.some(port =>
            [...port].some(edge => activeAudioInput(context, edge)))) return;
        if (node instanceof GainNode || node instanceof StereoPannerNode ||
            node instanceof ChannelSplitterNode || node instanceof ChannelMergerNode)
            releaseAudioVoiceNode(context, node);
        else if (node instanceof BiquadFilterNode || node instanceof IIRFilterNode)
            context.retiringVoices.add(node);
    };
    const retireAudioSource = node => {
        // A source's final samples may still be pulled by another port or a
        // feedback-delay writer during this quantum. Retire after all writes.
        audioContextState.get(node.context).completedVoiceSources.add(node);
    };
    const settleAudioVoiceTails = context => {
        for (const source of context.completedVoiceSources) releaseAudioVoiceNode(context, source);
        context.completedVoiceSources.clear();
        for (const node of context.retiringVoices) {
            if (audioNodeState.get(node).inputs.some(port =>
                [...port].some(edge => activeAudioInput(context, edge)))) {
                context.retiringVoices.delete(node);
                continue;
            }
            const tail = node instanceof BiquadFilterNode ?
                audioBiquadState.get(node).channels.some(biquadTailActive) :
                [...audioIirState.get(node).input, ...audioIirState.get(node).output]
                    .some(iirTailActive);
            if (!tail) releaseAudioVoiceNode(context, node);
        }
    };
    const activateAudioVoiceBranch = (context, node, added = []) => {
        if (!node || node instanceof AudioListener || context.nodes.has(node) ||
            audioScheduledState.get(node)?.ended) return added;
        if (context.nodes.size >= MAX_AUDIO_NODES)
            throw new AudioDOMException('Audio graph node limit reached', 'NotSupportedError');
        context.nodes.add(node);
        added.push(node);
        context.graphRevision++;
        for (const port of audioNodeState.get(node).outputs) for (const edge of port) {
            if (!edge.counted) { edge.counted = true; context.connections++; }
            const parameter = audioParamState.get(edge.destination);
            if (parameter) parameter.inputs.add(edge);
            else audioNodeState.get(edge.destination).inputs[edge.input].add(edge);
            activateAudioVoiceBranch(context,
                audioParamState.get(edge.destination)?.owner ?? edge.destination, added);
        }
        return added;
    };
