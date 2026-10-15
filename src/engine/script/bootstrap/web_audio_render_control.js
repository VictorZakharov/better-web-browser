    let audioRenderLastGraph = '';
    let audioRenderPollScheduled = false;
    const audioRenderRetire = retired => {
        const context = liveAudioContext;
        if (!context) return;
        const state = audioContextState.get(context);
        for (const [id, ended] of retired) {
            const reference = audioRenderObjects.get(id);
            const node = reference && derefAudioSource(reference);
            if (!node) { audioRenderObjects.delete(id); continue; }
            if (ended) {
                const source = audioScheduledState.get(node);
                if (!source.ended) {
                    source.ended = true;
                    if (node instanceof AudioBufferSourceNode) releaseBufferSourceContent(node);
                    audioTask(() => node.dispatchEvent(new AudioEvent('ended')));
                }
            }
            // Tail completion is decided by the rendering realm's actual DSP,
            // not the control realm's untouched filter/delay buffers.
            deactivateAudioVoiceNode(state, node);
            audioRenderObjects.delete(id);
        }
    };
    const pollAudioRender = () => {
        audioRenderPollScheduled = false;
        const context = liveAudioContext;
        if (!context) return;
        const live = audioContextState.get(context).live;
        const reply = audioHost('audioRenderPoll');
        if (reply) {
            const status = audioRenderParse(reply[0]);
            live.renderFrame = status.frame;
            for (const packet of status.packets) {
                audioRenderRetire(packet.retired);
                deliverAudioRenderCallbacks(context, packet.events, reply.slice(1), packet.arrayOffset);
            }
            if (status.error && live.phase === 'active') {
                console.error('Web Audio rendering stopped: ' + status.error);
                live.userSuspended = true;
                live.finalClose = true;
                settleLivePromises(live.resumePromises,
                    new AudioDOMException(status.error, 'NotSupportedError'));
                stopLiveStream(context);
            } else if (status.started && live.phase === 'active') {
                setAudioContextState(context, 'running');
                settleLivePromises(live.resumePromises);
            }
        }
        if (live.phase !== 'idle' && !audioRenderPollScheduled) {
            audioRenderPollScheduled = true;
            audioTask(pollAudioRender, 10);
        }
    };
    const startAudioRender = context => {
        const live = audioContextState.get(context).live;
        audioRenderLastGraph = '';
        if (!audioHost('audioRenderStart', live.streamId, context.sampleRate))
            throw new AudioDOMException('Audio rendering thread could not start', 'NotSupportedError');
        flushAudioRender();
        if (!audioRenderPollScheduled) {
            audioRenderPollScheduled = true;
            audioTask(pollAudioRender, 0);
        }
    };
    const flushAudioRender = () => {
        if (audioRenderThread || !liveAudioContext) return;
        const live = audioContextState.get(liveAudioContext).live;
        if (!live.threaded || live.phase !== 'active') return;
        const serialized = audioRenderStringify(audioRenderSnapshot(liveAudioContext));
        if (serialized === audioRenderLastGraph) return;
        if (!audioHost('audioRenderGraph', serialized))
            throw new AudioDOMException('Audio graph transfer exceeds the rendering limit', 'NotSupportedError');
        audioRenderLastGraph = serialized;
        audioRenderKnownAssets = audioRenderUsedAssets;
    };
    Object.defineProperty(globalThis, '__flushAudioRender', {
        configurable: true, value: flushAudioRender
    });
