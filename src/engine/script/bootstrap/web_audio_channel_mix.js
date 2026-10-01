    // Web Audio §4: only mono/stereo/quad/5.1 have defined speaker matrices.
    // Other layouts use positional discrete mapping, not a guessed surround
    // layout. Rows contain [source channel, gain] pairs; LFE is omitted in
    // every normative speaker downmix.
    const audioChannelMatrices = new Map();
    const audioChannelMatrix = (source, destination, interpretation) => {
        const key = `${source}:${destination}:${interpretation}`;
        if (audioChannelMatrices.has(key)) return audioChannelMatrices.get(key);
        const rows = Array.from({ length: destination }, () => []);
        const add = (target, input, gain = 1) => rows[target].push([input, gain]);
        const speakers = interpretation === 'speakers' &&
            [1, 2, 4, 6].includes(source) && [1, 2, 4, 6].includes(destination);
        if (!speakers || source === destination) {
            for (let i = 0; i < Math.min(source, destination); ++i) add(i, i);
        } else if (source === 1) {
            if (destination === 6) add(2, 0);
            else { add(0, 0); add(1, 0); }
        } else if (source < destination) {
            add(0, 0); add(1, 1);
            if (source === 4) { add(4, 2); add(5, 3); }
        } else if (destination === 1) {
            if (source === 2 || source === 4) {
                for (let i = 0; i < source; ++i) add(0, i, 1 / source);
            } else {
                add(0, 0, Math.SQRT1_2); add(0, 1, Math.SQRT1_2);
                add(0, 2); add(0, 4, 0.5); add(0, 5, 0.5);
            }
        } else if (source === 4) {
            add(0, 0, 0.5); add(0, 2, 0.5);
            add(1, 1, 0.5); add(1, 3, 0.5);
        } else {
            add(0, 0); add(0, 2, Math.SQRT1_2);
            add(1, 1); add(1, 2, Math.SQRT1_2);
            if (destination === 2) {
                add(0, 4, Math.SQRT1_2); add(1, 5, Math.SQRT1_2);
            } else { add(2, 4); add(3, 5); }
        }
        audioChannelMatrices.set(key, rows);
        return rows;
    };
    const sumAudioBus = (target, source, frames, interpretation) => {
        const matrix = audioChannelMatrix(source.length, target.length, interpretation);
        for (let channel = 0; channel < target.length; ++channel) {
            const output = target[channel];
            for (const [input, gain] of matrix[channel]) {
                const samples = source[input];
                for (let i = 0; i < frames; ++i) output[i] += samples[i] * gain;
            }
        }
    };
    const mixAudioBuses = (buses, channels, frames, interpretation) => {
        const output = silence(channels, frames);
        for (const bus of buses) sumAudioBus(output, bus, frames, interpretation);
        return output;
    };
    const resizeAudioHistories = (histories, count, create) => {
        while (histories.length < count) histories.push(create());
        histories.length = count;
    };
    const audioInputBuses = (context, node, frame, frames, cache, input = 0) =>
        [...audioNodeState.get(node).inputs[input]].map(edge =>
            renderAudioNode(context, edge.source, frame, frames, cache, edge.output));
    const mixAudioInputs = (context, node, frame, frames, cache,
        channels, input = 0, interpretation) => {
        const state = audioNodeState.get(node);
        const buses = audioInputBuses(context, node, frame, frames, cache, input);
        channels ??= computedAudioChannels(state, buses.map(bus => bus.length));
        return mixAudioBuses(buses, channels, frames,
            interpretation ?? state.channelInterpretation);
    };
