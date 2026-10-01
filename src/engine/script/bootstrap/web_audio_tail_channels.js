    // §4.3: filters must preserve a wider output while old channel histories
    // can still affect it. The wider prevailing input layout uses the node's
    // mixing rules; new histories start empty, not with another lane's state.
    // Up-mixing is performed before DSP, never after a nonlinear processor.
    const audioFilterInput = (node, input, histories, active, create) => {
        let channels = input.length;
        for (let channel = input.length; channel < histories.length; ++channel)
            if (active(histories[channel], channel)) channels = channel + 1;
        resizeAudioHistories(histories, channels, create);
        if (channels === input.length) return input;
        return mixAudioBuses([input], channels, input[0].length,
            audioNodeState.get(node).channelInterpretation);
    };
    const biquadTailActive = state => [state.x1, state.x2, state.y1, state.y2]
        .some(value => Number.isFinite(value) && Math.abs(value) >= Math.pow(2, -126));
    const iirTailActive = state => state.some(value =>
        Number.isFinite(value) && Math.abs(value) >= Math.pow(2, -126));
    // Finite-impulse processors can retire layouts exactly after their bounded
    // response. Track a deadline per width rather than retaining every quantum.
    const retainAudioTailWidth = (state, width, frame, frames, tailFrames) => {
        state.channelDeadlines ??= new Float64Array(33);
        state.channelDeadlines[width] = frame + frames + tailFrames;
        for (let channels = 32; channels > width; --channels)
            if (state.channelDeadlines[channels] > frame) return channels;
        return width;
    };
