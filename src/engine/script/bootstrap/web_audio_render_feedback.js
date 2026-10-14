    const refreshAudioRenderFeedback = node => {
        if (audioRenderThread || !audioContextState.get(node.context)?.live?.threaded) return;
        const reply = audioHost('audioRenderFeedback', audioRenderId(node));
        if (!reply) return;
        if (node instanceof DynamicsCompressorNode) {
            audioCompressorState.get(node).reduction = reply[1];
        } else {
            const state = analyserState.get(node);
            if (state.frame === reply[0]) return;
            const samples = new Float32Array(reply[1].buffer);
            state.history.fill(0);
            state.history.set(samples);
            state.cursor = samples.length % ANALYSER_HISTORY;
            state.frame = reply[0];
            state.frequencyFrame = reply[0];
            state.frequency = new Float32Array(reply[2].buffer);
        }
    };
    if (audioRenderThread) Object.defineProperty(globalThis, '__audioThreadFeedback', {
        configurable: true, value: ids => ids.map(id => {
            const node = audioThreadNode(id);
            if (node instanceof AnalyserNode) {
                const state = analyserState.get(node);
                return [id, state.frame, new Uint8Array(analyserTimeData(state).buffer),
                    new Uint8Array(analyserFrequencyData(state).buffer)];
            }
            if (node instanceof DynamicsCompressorNode)
                return [id, audioContextState.get(node.context).live.renderFrame,
                    audioCompressorState.get(node).reduction];
            return [id];
        })
    });
