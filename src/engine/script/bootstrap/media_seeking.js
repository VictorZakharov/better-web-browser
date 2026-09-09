    // HTML seeking + MSE 3.15.3: a seek outside buffered data waits at the requested
    // position until every active track covers it. The native decoder only owns
    // appended bytes; its current end is not the end of the MSE presentation.
    // https://www.w3.org/TR/media-source-2/#mediasource-seeking
    const pendingMediaSeeks = new WeakMap();
    const mediaSeekHasData = (element, target) => {
        const state = mediaStateFor(element);
        for (let i = 0; i < state.buffered.length; i++) {
            if (state.buffered.start(i) <= target && (target < state.buffered.end(i)
                || target === state.duration && target === state.buffered.end(i))) return true;
        }
        return false;
    };
    const continueMediaSourceSeek = element => {
        const seek = pendingMediaSeeks.get(element);
        if (!seek || seek.sent || !mediaSeekHasData(element, seek.target)) return;
        seek.sent = true;
        traceMediaLifecycle(element, 'seek:ready');
        const requestId = nextMediaRequest++;
        pendingMediaRequests.set(requestId, { seek });
        mediaCommand(element, requestId, 'seek', seek.target);
    };
    const beginMediaSourceSeek = (element, buffering = false) => {
        const source = mediaSourceForElement.get(element);
        if (!source) return false;
        const state = mediaStateFor(element);
        const previous = pendingMediaSeeks.get(element);
        const seek = { target: state.currentTime, sent: false, buffering,
            playback: previous?.playback || [] };
        pendingMediaSeeks.set(element, seek);
        state.ended = false;
        // Suspend the native clock without changing the author's paused state or
        // exposing an artificial pause event. Generation-tag the acknowledgement.
        const requestId = nextMediaRequest++;
        pendingMediaRequests.set(requestId, { seekPause: true });
        mediaCommand(element, requestId, 'playback', false, effectiveVolumeMillis(state));
        if (!mediaSeekHasData(element, seek.target)) {
            const wasPlaying = !state.paused && state.readyState >= HTMLMediaElement.HAVE_FUTURE_DATA;
            state.readyState = buffering ? HTMLMediaElement.HAVE_CURRENT_DATA : HTMLMediaElement.HAVE_METADATA;
            traceMediaLifecycle(element, buffering ? 'buffer:waiting' : 'seek:waiting');
            if (wasPlaying) {
                queueMediaEvent(element, 'timeupdate');
                queueMediaEvent(element, 'waiting');
            }
        }
        continueMediaSourceSeek(element);
        return true;
    };
    // MSE SourceBuffer Monitoring: exhausting either active track blocks the
    // shared playback clock. Audio buffered farther ahead must not run alone.
    // Reuse generation-tagged seek recovery to discard stale clock replies and
    // restore the held position once data arrives, without exposing seek events.
    // https://www.w3.org/TR/media-source-2/#sourcebuffer-monitoring
    const monitorMediaSourcePlayback = (element, position) => {
        const source = mediaSourceForElement.get(element);
        const state = mediaStateFor(element);
        if (!source || state.paused || state.error || pendingMediaSeeks.has(element)) return false;
        position = Math.max(0, Number(position) || 0);
        for (let i = 0; i < state.buffered.length; i++) {
            const start = state.buffered.start(i), end = state.buffered.end(i);
            if (start <= state.currentTime && state.currentTime <= end
                && position >= end && end < state.duration) {
                state.currentTime = end;
                beginMediaSourceSeek(element, true);
                return true;
            }
        }
        return false;
    };
    const deferSeekingPlayback = (element, requestId) => {
        const seek = pendingMediaSeeks.get(element);
        if (!seek) return false;
        const state = mediaStateFor(element);
        if (state.paused) {
            state.paused = false;
            queueMediaEvent(element, 'play');
        }
        seek.playback.push(requestId);
        return true;
    };
    const rejectSeekingPlayback = element => {
        const seek = pendingMediaSeeks.get(element);
        for (const requestId of seek?.playback.splice(0) || []) {
            pendingMediaRequests.get(requestId)?.reject(
                new DOMException('Playback was interrupted', 'AbortError'));
            pendingMediaRequests.delete(requestId);
        }
    };
    const cancelMediaSourceSeek = element => {
        rejectSeekingPlayback(element);
        pendingMediaSeeks.delete(element);
        mediaStateFor(element).seeking = false;
    };
    const applyMediaSeekResponse = (element, input, pending) => {
        const state = mediaStateFor(element);
        if (input.disposition === 'media-error') {
            cancelMediaSourceSeek(element);
            return false;
        }
        if (pending?.seekPause) return true;
        if (pending?.seek) {
            const seek = pending.seek;
            if (pendingMediaSeeks.get(element) !== seek) return true;
            if (input.disposition !== 'seeked') {
                cancelMediaSourceSeek(element);
                return false;
            }
            pendingMediaSeeks.delete(element);
            state.currentTime = Math.max(0, Number(input.currentTime) || 0);
            state.seeking = false;
            if (!seek.buffering) state.readyState = HTMLMediaElement.HAVE_CURRENT_DATA;
            updateMediaCanPlay(element);
            queueMediaEvent(element, 'timeupdate');
            if (!seek.buffering) queueMediaEvent(element, 'seeked');
            const requests = seek.playback.length ? seek.playback : [0];
            if (!state.paused) {
                for (const requestId of requests)
                    mediaCommand(element, requestId, 'playback', true, effectiveVolumeMillis(state));
            }
            return true;
        }
        // Clock/EOF notifications from the old position must not undo a pending
        // seek while the page fetches and appends data for the new position.
        if (pendingMediaSeeks.has(element) && input.disposition === 'playing') {
            if (pending?.resolve) {
                pendingMediaRequests.set(Number(input.requestId), pending);
                pendingMediaSeeks.get(element).playback.push(Number(input.requestId));
            }
            return true;
        }
        return pendingMediaSeeks.has(element)
            && (input.disposition === 'time' || input.disposition === 'ended');
    };
