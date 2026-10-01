    // Web Audio AudioNodeOptions and per-interface channel constraints. Keep the
    // conversion snapshot separate from graph admission: rejected constructors
    // must not consume a node slot or retain an author dictionary.
    const audioOptionsDictionary = (context, input) => {
        if (!audioContextState.has(context))
            throw new TypeError('AudioNode requires a BaseAudioContext');
        if (input != null && typeof input !== 'object' && typeof input !== 'function')
            throw new TypeError('AudioNode options must be a dictionary');
        return input ?? {};
    };
    const readAudioChannelOptions = (context, input) => {
        const options = audioOptionsDictionary(context, input);
        // Web IDL visits inherited dictionary members in lexical order, and
        // completes each conversion before invoking the next author getter.
        const count = audioOption(options, 'channelCount', undefined, audioChannelUnsigned);
        const mode = audioOption(options, 'channelCountMode', undefined, value =>
            audioChannelEnum(value, ['max', 'clamped-max', 'explicit'], 'channelCountMode'));
        const interpretation = audioOption(options, 'channelInterpretation', undefined,
            value => audioChannelEnum(value, ['speakers', 'discrete'], 'channelInterpretation'));
        return { options, count, mode, interpretation };
    };
    const audioOption = (options, name, fallback, convert) => {
        const value = options[name];
        return value === undefined ? fallback : convert(value);
    };
    const audioChannelUnsigned = value => {
        // Unary conversion follows Web IDL ToNumber, unlike Number(BigInt).
        const number = +value;
        return Number.isFinite(number) ? Math.trunc(number) >>> 0 : 0;
    };
    const audioChannelEnum = (value, values, name) => {
        const converted = `${value}`;
        if (!values.includes(converted)) throw new TypeError('Invalid ' + name);
        return converted;
    };
    const audioEnumAttribute = (input, values) => {
        const value = `${input}`;
        return values.includes(value) ? value : null;
    };
    const audioChannelSettings = (context, snapshot, kind = 'generic', fixedCount) => {
        const restricted = ['convolver', 'compressor', 'panner', 'stereo-panner'].includes(kind);
        const fixed = kind === 'splitter' || kind === 'merger' || kind === 'destination';
        const state = {
            kind, fixedCount,
            channelCount: fixedCount ?? (kind === 'merger' ? 1 : 2),
            channelCountMode: fixed ? 'explicit' : restricted ? 'clamped-max' : 'max',
            channelInterpretation: kind === 'splitter' ? 'discrete' : 'speakers',
            restricted,
            offlineDestination: kind === 'destination' &&
                Number.isFinite(audioContextState.get(context).length)
        };
        for (const [member, value] of [
            ['channelCount', snapshot.count], ['channelCountMode', snapshot.mode],
            ['channelInterpretation', snapshot.interpretation]
        ]) if (value !== undefined) validateAudioChannelSetting(state, member, value);
        return state;
    };
    const validateAudioChannelSetting = (state, member, value) => {
        const fail = (message, name) => { throw new AudioDOMException(message, name); };
        if (member === 'channelCount') {
            if (state.kind === 'script-processor' && value !== state.channelCount)
                fail('ScriptProcessor has a fixed input channel count', 'NotSupportedError');
            if (state.kind === 'script-processor') return;
            if (state.kind === 'destination' && !state.offlineDestination &&
                (value < 1 || value > state.fixedCount))
                fail('Channel count exceeds the output device', 'IndexSizeError');
            if ((state.kind === 'splitter' || state.kind === 'merger' ||
                state.offlineDestination) && value !== state.channelCount)
                fail('This node has a fixed channel count', 'InvalidStateError');
            if (value < 1 || value > 32)
                fail('Channel count must be between 1 and 32', 'NotSupportedError');
            if (state.restricted && value > 2)
                fail('This node accepts at most two input channels', 'NotSupportedError');
        } else if (member === 'channelCountMode') {
            if (state.kind === 'script-processor' && value !== 'explicit')
                fail('ScriptProcessor requires explicit channel count', 'NotSupportedError');
            if ((state.kind === 'splitter' || state.kind === 'merger' ||
                state.offlineDestination) && value !== 'explicit')
                fail('This node requires explicit channel count', 'InvalidStateError');
            if (state.restricted && value === 'max')
                fail('This node cannot use max channel count', 'NotSupportedError');
        } else if (state.kind === 'splitter' && value !== 'discrete') {
            fail('ChannelSplitterNode requires discrete interpretation', 'InvalidStateError');
        }
        state[member] = value;
    };
    const ownedAudioNodeState = node => {
        const state = audioNodeState.get(node);
        if (!state) throw new TypeError('Illegal AudioNode receiver');
        return state;
    };
    const setAudioChannelSetting = (node, member, input) => {
        const state = ownedAudioNodeState(node);
        const value = member === 'channelCount' ? audioChannelUnsigned(input) : `${input}`;
        // Web IDL enum attribute setters ignore unrecognized strings, unlike
        // dictionary conversion. ToString errors still propagate unchanged.
        if (member !== 'channelCount' && !(member === 'channelCountMode' ?
            ['max', 'clamped-max', 'explicit'] : ['speakers', 'discrete']).includes(value)) return;
        const previous = state[member];
        validateAudioChannelSetting(state, member, value);
        if (previous === value) return;
        const context = audioContextState.get(node.context);
        context.graphRevision++;
        if (context.renderStarted && !audioWorkWithinLimit(context, context.nodes.size)) {
            state[member] = previous;
            context.graphRevision++;
            throw new AudioDOMException('Audio channel change exceeds the rendering work limit',
                'NotSupportedError');
        }
    };
    const computedAudioChannels = (state, widths) => {
        const maximum = Math.max(1, ...widths);
        return state.channelCountMode === 'explicit' ? state.channelCount :
            state.channelCountMode === 'clamped-max' ? Math.min(state.channelCount, maximum) : maximum;
    };
