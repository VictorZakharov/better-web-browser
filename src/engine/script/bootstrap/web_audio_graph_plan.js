    // The bounded routing graph includes AudioParam and listener dependencies.
    // Cycles are legal connections: §1.5.5/§2.4 split cyclic DelayNodes into a
    // previous-quantum reader and a writer, then mute only remaining cycles.
    const audioGraphEdges = context => {
        const state = audioContextState.get(context);
        const edges = [];
        for (const node of state.nodes) {
            for (const port of audioNodeState.get(node).outputs) for (const edge of port) {
                const parameter = audioParamState.get(edge.destination);
                edges.push({ source: node, destination: parameter?.owner ?? edge.destination,
                    parameter: !!parameter });
            }
        }
        for (const panner of state.panners)
            edges.push({ source: context.listener, destination: panner, parameter: true });
        return edges;
    };
    const cyclicAudioVertices = (vertices, edges) => {
        const successors = new Map(vertices.map(node => [node, []]));
        for (const edge of edges) successors.get(edge.source).push(edge.destination);
        const indices = new Map(), low = new Map(), active = new Set(), stack = [];
        const cyclic = new Set();
        let next = 0;
        const visit = node => {
            indices.set(node, next);
            low.set(node, next++);
            stack.push(node);
            active.add(node);
            for (const child of successors.get(node)) {
                if (!indices.has(child)) {
                    visit(child);
                    low.set(node, Math.min(low.get(node), low.get(child)));
                } else if (active.has(child))
                    low.set(node, Math.min(low.get(node), indices.get(child)));
            }
            if (low.get(node) !== indices.get(node)) return;
            const component = [];
            let member;
            do {
                member = stack.pop();
                active.delete(member);
                component.push(member);
            } while (member !== node);
            if (component.length > 1 || successors.get(node).includes(node))
                for (const vertex of component) cyclic.add(vertex);
        };
        for (const node of vertices) if (!indices.has(node)) visit(node);
        return cyclic;
    };
    const audioOutputWidth = (node, input) => {
        if (node instanceof ScriptProcessorNode)
            return Math.max(1, scriptProcessorState(node).outputChannels);
        if (node instanceof ChannelSplitterNode) return 1;
        if (node instanceof ChannelMergerNode) return node.numberOfInputs;
        if (node instanceof PannerNode || node instanceof StereoPannerNode) return 2;
        if (node instanceof AudioBufferSourceNode) {
            const source = audioBufferSourceState.get(node);
            return source.acquired?.numberOfChannels ?? source.buffer?.numberOfChannels ?? 1;
        }
        if (node instanceof AudioScheduledSourceNode) return 1;
        if (node instanceof MediaStreamAudioSourceNode)
            return audioCaptureState.get(node).channels || 1;
        if (node instanceof ConvolverNode) {
            const impulse = audioConvolverState.get(node).buffer;
            return impulse ? Math.max(input, Math.min(2, impulse.numberOfChannels)) : 1;
        }
        return input;
    };
    const audioGraphWidths = context => {
        const nodes = [...audioContextState.get(context).nodes];
        const widths = new Map(nodes.map(node => [node, 1]));
        // Width transfer functions are monotone and bounded to [1, 32]. The
        // least fixed point handles feedback without recursive bus inspection.
        for (let pass = 0; pass <= nodes.length; ++pass) {
            let changed = false;
            for (const node of nodes) {
                const state = audioNodeState.get(node);
                const input = computedAudioChannels(state, [...(state.inputs[0] ?? [])]
                    .map(edge => widths.get(edge.source)));
                const width = audioOutputWidth(node, input);
                if (width !== widths.get(node)) { widths.set(node, width); changed = true; }
            }
            if (!changed) break;
        }
        return widths;
    };
    const audioQuantumPlan = context => {
        const state = audioContextState.get(context);
        if (state.graphPlan?.revision === state.graphRevision) return state.graphPlan;
        const vertices = [...state.nodes, context.listener];
        const edges = audioGraphEdges(context);
        const original = cyclicAudioVertices(vertices, edges);
        const delays = new Set([...original].filter(node => node instanceof DelayNode));
        const muted = cyclicAudioVertices(vertices, edges.filter(edge =>
            edge.parameter || !delays.has(edge.destination)));
        const widths = audioGraphWidths(context);
        return state.graphPlan = { delays, muted, widths, revision: state.graphRevision };
    };
    const audioGraphWork = (contextState, proposedNodes = contextState.nodes.size) => {
        const widths = audioQuantumPlan(contextState.context).widths;
        const modulationWork = new Map();
        for (const source of contextState.nodes)
            for (const port of audioNodeState.get(source).outputs)
                for (const edge of port) {
                    const param = audioParamState.get(edge.destination);
                    if (param) modulationWork.set(param.owner,
                        (modulationWork.get(param.owner) || 0) + widths.get(source));
                }
        // Preserve the existing conservative per-node floor while closing the
        // mono-destination loophole: wide intermediate buses, splitter ports,
        // fan-in, and retained processor tails all consume real work.
        let work = Math.max(0, proposedNodes - contextState.nodes.size) * contextState.channels;
        for (const node of contextState.nodes) {
            const state = audioNodeState.get(node);
            let width = widths.get(node);
            if (node instanceof DelayNode) width = Math.max(width, audioDelayState.get(node).ring.length);
            if (node instanceof IIRFilterNode) width = Math.max(width, audioIirState.get(node).output.length);
            if (node instanceof BiquadFilterNode) width = Math.max(width, audioBiquadState.get(node).channels.length);
            if (node instanceof WaveShaperNode)
                width = Math.max(width, audioWaveShaperState.get(node).oversampleDsp?.lanes.length || 0);
            // Include input matrix work even when a wide source downmixes to
            // a narrow processor. AudioParam edges are separate from inputs.
            const mixing = state.inputs.reduce((cost, port) => cost + [...port]
                .reduce((sum, edge) => sum + Math.max(width, widths.get(edge.source)), 0), 0);
            const portWork = Math.max(width * Math.max(1, state.outputs.length), mixing) +
                (modulationWork.get(node) || 0);
            work += Math.max(contextState.channels, portWork);
        }
        work += modulationWork.get(contextState.context.listener) || 0;
        return work;
    };
