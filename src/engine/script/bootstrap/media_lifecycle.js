    // HTML load cancels the old resource's tasks; MSE detachment invalidates its
    // SourceBuffers, not merely the media element's displayed state.
    // https://html.spec.whatwg.org/multipage/media.html#concept-media-load-algorithm
    // https://www.w3.org/TR/media-source-2/#mediasource-detach
    const mediaLoadGeneration = new WeakMap();
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
        mediaCommand(element, 0, 'reset');
        if (hadResource) queueMediaEvent(element, 'emptied');
        if (positionChanged) queueMediaEvent(element, 'timeupdate');
    };
    const selectMediaSource = element => {
        const value = element.getAttribute('src');
        const source = objectUrlValue(value);
        if (!(source instanceof MediaSource)) return;
        if (source.readyState !== 'closed') {
            const state = mediaStateFor(element);
            state.error = new MediaError(MediaError.MEDIA_ERR_SRC_NOT_SUPPORTED,
                'The MediaSource is already attached');
            state.networkState = HTMLMediaElement.NETWORK_NO_SOURCE;
            queueMediaEvent(element, 'error');
            return;
        }
        const state = mediaStateFor(element);
        state.networkState = HTMLMediaElement.NETWORK_LOADING;
        state.currentSrc = value;
        queueMediaEvent(element, 'loadstart');
        source.__attach(element);
    };
    const mediaSourceAttributeChanged = element => {
        if (mediaStateFor(element).networkState !== HTMLMediaElement.NETWORK_EMPTY
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
