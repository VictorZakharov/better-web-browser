    // HTML load cancels the old resource's tasks; MSE detachment invalidates its
    // SourceBuffers, not merely the media element's displayed state.
    // https://html.spec.whatwg.org/multipage/media.html#concept-media-load-algorithm
    // https://www.w3.org/TR/media-source-2/#mediasource-detach
    const mediaLoadGeneration = new WeakMap();
    const hasOrdinaryMediaSource = element => element.hasAttribute('src')
        || element.querySelector('source') !== null;
    const beginOrdinaryMediaLoad = element => {
        const state = mediaStateFor(element);
        if (state.networkState !== HTMLMediaElement.NETWORK_EMPTY
            || !hasOrdinaryMediaSource(element)) return;
        state.networkState = HTMLMediaElement.NETWORK_LOADING;
        queueMediaEvent(element, 'loadstart');
    };
    const rejectDeferredMediaPlayback = (element, error) => {
        for (const [id, request] of pendingMediaRequests) {
            if (request.element !== element || !request.deferredOrdinaryPlayback) continue;
            pendingMediaRequests.delete(id);
            request.reject(error);
        }
    };
    const startDeferredMediaPlayback = element => {
        const state = mediaStateFor(element);
        const volumeMillis = effectiveVolumeMillis(state);
        let started = false;
        for (const [id, request] of pendingMediaRequests) {
            if (request.element !== element || !request.deferredOrdinaryPlayback) continue;
            request.deferredOrdinaryPlayback = false;
            mediaCommand(element, id, 'playback', true, volumeMillis);
            started = true;
        }
        return started;
    };
    const applyOrdinaryMediaSourceEvent = input => {
        const element = wrap(Number(input.target) || 0);
        if (!(element instanceof HTMLMediaElement) || mediaSourceForElement.has(element)) return false;
        const state = mediaStateFor(element);
        if (state.srcObject || state.awaitingSourceReset) return true;
        switch (input.disposition) {
            case 'loading':
                beginOrdinaryMediaLoad(element);
                return true;
            case 'selected':
                beginOrdinaryMediaLoad(element);
                state.currentSrc = String(input.sourceUrl || '');
                return true;
            case 'error':
                rejectDeferredMediaPlayback(element,
                    new DOMException('The media resource could not be loaded', 'NotSupportedError'));
                state.error = new MediaError(MediaError.MEDIA_ERR_SRC_NOT_SUPPORTED,
                    'The media resource could not be loaded');
                state.networkState = HTMLMediaElement.NETWORK_NO_SOURCE;
                queueMediaEvent(element, 'error');
                return true;
            case 'waiting':
                rejectDeferredMediaPlayback(element,
                    new DOMException('No supported media source is available', 'NotSupportedError'));
                state.networkState = HTMLMediaElement.NETWORK_NO_SOURCE;
                return true;
            default:
                return false;
        }
    };
    const detachMediaSource = element => {
        const source = mediaSourceForElement.get(element);
        if (!source) return;
        mediaSourceForElement.delete(element);
        source.__element = null;
        source.readyState = 'closed';
        source.__duration = NaN;
        source.activeSourceBuffers.__replace([]);
        queueMediaEvent(source.activeSourceBuffers, 'removesourcebuffer');
        for (const buffer of source.sourceBuffers) buffer.__detach();
        source.sourceBuffers.__replace([]);
        queueMediaEvent(source.sourceBuffers, 'removesourcebuffer');
        source.__committing = source.__loadedState = false;
        source.__commitBuffers = [];
        source.__pendingPlayback = [];
        source.__encodedBytes = 0;
        queueMediaEvent(source, 'sourceclose');
    };
    const resetMediaElement = element => {
        const state = mediaStateFor(element);
        mediaLoadGeneration.set(element, (mediaLoadGeneration.get(element) || 0) + 1);
        // Every command carries a request identity. Late replies from the old
        // resource must not change the replacement's clock, ranges, or promises.
        state.requestFloor = nextMediaRequest;
        cancelMediaSourceSeek(element);
        for (const [id, request] of pendingMediaRequests) {
            if (request.element !== element) continue;
            pendingMediaRequests.delete(id);
            request.reject?.(new DOMException('The media resource was replaced', 'AbortError'));
        }
        const hadResource = state.networkState !== HTMLMediaElement.NETWORK_EMPTY;
        const positionChanged = state.currentTime !== 0;
        if (state.networkState === HTMLMediaElement.NETWORK_LOADING
            || state.networkState === HTMLMediaElement.NETWORK_IDLE) queueMediaEvent(element, 'abort');
        detachMediaSource(element);
        clearDecodedAudioTrack(element);
        clearDecodedVideoTrack(element);
        state.audioTrackEnabled = true;
        state.networkState = HTMLMediaElement.NETWORK_EMPTY;
        state.readyState = HTMLMediaElement.HAVE_NOTHING;
        state.error = null;
        state.currentSrc = '';
        state.duration = NaN;
        state.currentTime = 0;
        state.paused = true;
        state.ended = state.seeking = false;
        state.buffered = emptyTimeRanges();
        state.seekable = emptyTimeRanges();
        state.played = emptyTimeRanges();
        state.videoWidth = state.videoHeight = 0;
        state.playbackRate = state.defaultPlaybackRate;
        const resetRequestId = nextMediaRequest++;
        if (state.awaitingSourceReset) state.awaitingSourceReset = resetRequestId;
        mediaCommand(element, resetRequestId, 'reset');
        updateTextTracks(element);
        if (hadResource) queueMediaEvent(element, 'emptied');
        if (positionChanged) queueMediaEvent(element, 'timeupdate');
        return resetRequestId;
    };
    const selectMediaSource = element => {
        const state = mediaStateFor(element);
        if (state.srcObject) {
            state.duration = Infinity;
            state.networkState = HTMLMediaElement.NETWORK_IDLE;
            queueMediaEvent(element, 'loadstart');
            globalThis.__attachCaptureStream?.(element, nodeId(element));
            return;
        }
        const value = element.getAttribute('src');
        const source = objectUrlValue(value);
        if (!(source instanceof MediaSource)) {
            beginOrdinaryMediaLoad(element);
            return;
        }
        if (source.readyState !== 'closed') {
            state.error = new MediaError(MediaError.MEDIA_ERR_SRC_NOT_SUPPORTED,
                'The MediaSource is already attached');
            state.networkState = HTMLMediaElement.NETWORK_NO_SOURCE;
            queueMediaEvent(element, 'error');
            return;
        }
        state.networkState = HTMLMediaElement.NETWORK_LOADING;
        state.currentSrc = value;
        queueMediaEvent(element, 'loadstart');
        source.__attach(element);
    };
    const restartMediaLoad = element => {
        if (!mediaStateFor(element).srcObject
            && !(objectUrlValue(element.getAttribute('src')) instanceof MediaSource))
            mediaCommand(element, 0, 'reload');
        selectMediaSource(element);
    };
    const mediaSourceAttributeChanged = element => {
        const state = mediaStateFor(element);
        if (state.srcObject)
            globalThis.__detachCaptureStream?.(element, nodeId(element));
        if (state.srcObject || state.networkState !== HTMLMediaElement.NETWORK_EMPTY
            || mediaSourceForElement.has(element)
            || [...pendingMediaRequests.values()].some(request => request.element === element))
            resetMediaElement(element);
        else mediaLoadGeneration.set(element, (mediaLoadGeneration.get(element) || 0) + 1);
        selectMediaSource(element);
    };
    Object.defineProperty(HTMLMediaElement.prototype, 'src', {
        configurable: true,
        get() {
            const value = this.getAttribute('src');
            if (value == null) return '';
            return objectUrlEntries.has(value) ? value : host('resolveUrl', value);
        },
        set(value) { this.setAttribute('src', String(value)); }
    });
