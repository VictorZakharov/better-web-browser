    // AudioNode connection overloads (§1.5.5). Resolve the branded destination
    // before numeric conversion; AudioParam overloads do not read a third arg.
    const audioConnectionDestination = value => {
        const node = audioNodeState.get(value);
        const param = audioParamState.get(value);
        if (!node && !param) throw new TypeError('Destination must be an AudioNode or AudioParam');
        return { node, param, context: param?.context ?? value.context };
    };
    const audioPortIndex = (value, count, name) => {
        const index = audioChannelUnsigned(value);
        if (index >= count) throw new AudioDOMException(name + ' index is out of range',
            'IndexSizeError');
        return index;
    };
    const removeAudioEdge = (context, edge) => {
        audioNodeState.get(edge.source).outputs[edge.output].delete(edge);
        const param = audioParamState.get(edge.destination);
        if (param) param.inputs.delete(edge);
        else audioNodeState.get(edge.destination).inputs[edge.input].delete(edge);
        context.connections--;
        context.graphRevision++;
    };
    const connectAudioNode = (source, destination, output, input) => {
        const state = ownedAudioNodeState(source);
        const target = audioConnectionDestination(destination);
        // Complete Web IDL conversions before the operation's context checks.
        output = audioChannelUnsigned(output);
        input = target.param ? 0 : audioChannelUnsigned(input);
        if (target.context !== source.context)
            throw new AudioDOMException('Cannot connect different audio contexts', 'InvalidAccessError');
        audioPortIndex(output, state.outputs.length, 'Output');
        if (!target.param) audioPortIndex(input, target.node.inputs.length, 'Input');
        const port = state.outputs[output];
        if ([...port].some(edge => edge.destination === destination && edge.input === input))
            return target.param ? undefined : destination;
        const context = audioContextState.get(source.context);
        if (context.connections >= MAX_AUDIO_CONNECTIONS)
            throw new AudioDOMException('Audio graph connection limit reached', 'NotSupportedError');
        const edge = { source, destination, output, input };
        port.add(edge);
        if (target.param) target.param.inputs.add(edge);
        else target.node.inputs[input].add(edge);
        context.connections++;
        context.graphRevision++;
        // Roll back both endpoint sets and budget ownership if a live graph
        // edit cannot be rendered. A failed call must not change audible PCM.
        if (context.renderStarted && !audioWorkWithinLimit(context, context.nodes.size)) {
            removeAudioEdge(context, edge);
            throw new AudioDOMException('Audio connection exceeds the rendering work limit',
                'NotSupportedError');
        }
        return target.param ? undefined : destination;
    };
    const disconnectAudioNode = (source, args) => {
        const state = ownedAudioNodeState(source);
        const context = audioContextState.get(source.context);
        let destination, target, output, input;
        if (args.length) {
            const first = args[0];
            if (audioNodeState.has(first) || audioParamState.has(first)) {
                destination = first;
                target = audioConnectionDestination(first);
                if (args.length > 1) output = audioChannelUnsigned(args[1]);
                if (args.length > 2 && !target.param) input = audioChannelUnsigned(args[2]);
                if (target.context !== source.context)
                    throw new AudioDOMException('Cannot disconnect different audio contexts',
                        'InvalidAccessError');
            } else if (args.length === 1) output = audioChannelUnsigned(first);
            else throw new TypeError('Destination must be an AudioNode or AudioParam');
        }
        if (output !== undefined) audioPortIndex(output, state.outputs.length, 'Output');
        if (input !== undefined) audioPortIndex(input, target.node.inputs.length, 'Input');
        const selected = state.outputs.flatMap((port, index) =>
            output !== undefined && index !== output ? [] : [...port].filter(edge =>
                (!destination || edge.destination === destination) &&
                (input === undefined || edge.input === input)));
        if (destination && !selected.length)
            throw new AudioDOMException('No connection to destination', 'InvalidAccessError');
        for (const edge of selected) removeAudioEdge(context, edge);
    };
