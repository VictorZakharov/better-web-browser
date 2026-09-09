    // A deliberately bounded Media Source implementation. It supports the two SourceBuffer
    // configurations required by MSE: one muxed buffer, or separate video and audio buffers.
    // Complete encoded media segments cross the contained-media boundary as bounded batches.
    const MAX_MEDIA_SOURCE_BYTES = 8 * 1024 * 1024;
    const objectUrlEntries = new Map();
    const mediaSourceForElement = new WeakMap();
    let nextObjectUrl = 1;
    const copyMediaBytes = value => {
        if (value instanceof ArrayBuffer) return new Uint8Array(value.slice(0));
        if (ArrayBuffer.isView?.(value))
            return new Uint8Array(value.buffer.slice(value.byteOffset, value.byteOffset + value.byteLength));
        return null;
    };
    const concatMediaBytes = chunks => {
        const size = chunks.reduce((total, chunk) => total + chunk.byteLength, 0);
        const output = new Uint8Array(size);
        let offset = 0;
        for (const chunk of chunks) {
            output.set(chunk, offset);
            offset += chunk.byteLength;
        }
        return output;
    };
    const releaseInternalMediaBytes = bytes => {
        // __hostCall copied this Breeze-owned transfer view into a Rust Vec synchronously. Detach
        // only this internal buffer so V8 can release its backing store without changing the
        // caller-owned appendBuffer input required by MSE.
        if (ArrayBuffer.isView(bytes) && bytes.buffer.byteLength)
            host('arrayBufferDetach', bytes.buffer);
    };
    const mediaTrackKind = type => {
        const source = String(type).toLowerCase();
        const video = source.includes('avc1.');
        const audio = source.includes('mp4a.40.2');
        if (video && audio) return 'muxed';
        if (video) return 'video';
        if (audio) return 'audio';
        return '';
    };
    const completeMediaSegmentPrefix = bytes => {
        const read32 = offset => ((bytes[offset] << 24) | (bytes[offset + 1] << 16)
            | (bytes[offset + 2] << 8) | bytes[offset + 3]) >>> 0;
        let offset = 0;
        let completeLength = 0;
        let initializationLength = 0;
        let foundMediaData = false;
        let waitingForMediaData = false;
        while (offset + 8 <= bytes.byteLength) {
            let size = read32(offset);
            let header = 8;
            if (size === 1) {
                if (offset + 16 > bytes.byteLength)
                    return { length: completeLength, initializationLength,
                        hasMediaData: foundMediaData, invalid: false };
                const high = read32(offset + 8);
                const low = read32(offset + 12);
                size = high * 0x100000000 + low;
                header = 16;
                if (!Number.isSafeInteger(size))
                    return { length: 0, initializationLength: 0,
                        hasMediaData: false, invalid: true };
            } else if (size === 0) {
                size = bytes.byteLength - offset;
            }
            if (size < header) return { length: 0, initializationLength: 0,
                hasMediaData: false, invalid: true };
            if (offset + size > bytes.byteLength)
                return { length: completeLength, initializationLength,
                    hasMediaData: foundMediaData, invalid: false };
            const kind = String.fromCharCode(
                bytes[offset + 4], bytes[offset + 5], bytes[offset + 6], bytes[offset + 7]
            );
            offset += size;
            if (kind === 'moof') {
                // ISO-BMFF MSE media segments are a moof followed by one or more mdat boxes.
                // A complete moof is not a safe prefix while its referenced mdat is partial.
                waitingForMediaData = true;
            } else if (kind === 'moov' && !foundMediaData) {
                initializationLength = offset;
            } else if (kind === 'mdat') {
                foundMediaData = true;
                waitingForMediaData = false;
                completeLength = offset;
            } else if (!waitingForMediaData && foundMediaData) {
                // Boxes between complete media segments may travel with the preceding segment.
                completeLength = offset;
            }
        }
        return {
            length: waitingForMediaData ? completeLength : Math.max(completeLength, offset),
            initializationLength,
            hasMediaData: foundMediaData,
            invalid: false
        };
    };

    const mediaSourceTypeSupported = type => {
        const source = String(type).toLowerCase();
        return supportedMediaType(source) === 'probably' && mediaTrackKind(source) !== '';
    };
    const objectUrlValue = url => objectUrlEntries.get(String(url));
    const createObjectUrl = value => {
        if (!(value instanceof MediaSource) && !(value instanceof Blob))
            throw new TypeError('URL.createObjectURL requires a Blob or MediaSource');
        const origin = parseUrl(currentUrl).origin;
        const url = 'blob:' + origin + '/' + (nextObjectUrl++).toString(36);
        objectUrlEntries.set(url, value);
        return url;
    };
    const revokeObjectUrl = url => { objectUrlEntries.delete(String(url)); };
    const queueMediaEvent = (target, name) => queueMicrotask(() =>
        target.dispatchEvent(markTrusted(new Event(name))));


    class MediaSource extends EventTarget {
        constructor() {
            super();
            this.readyState = 'closed';
            this.__duration = NaN;
            this.sourceBuffers = new SourceBufferList();
            this.activeSourceBuffers = new SourceBufferList();
            this.__element = null;
            this.__encodedBytes = 0;
            this.__committing = false;
            this.__commitBuffers = [];
            this.__loadedState = false;
            this.__waiting = false;
            this.__pendingPlayback = [];
        }
        static isTypeSupported(type) { return mediaSourceTypeSupported(type); }
        get duration() { return this.__duration; }
        set duration(value) {
            value = Number(value);
            if (Number.isNaN(value) || value < 0) throw new TypeError('Invalid media duration');
            if (this.readyState !== 'open' || [...this.sourceBuffers].some(buffer => buffer.updating))
                throw new DOMException('The MediaSource cannot change duration now', 'InvalidStateError');
            // The contained decoder currently reports range ends, not individual coded-frame
            // presentation timestamps. Reject truncation conservatively until those are exposed.
            const end = Math.max(0, ...[...this.sourceBuffers].flatMap(buffer =>
                buffer.__ranges.map(range => range[1])));
            if (value < end)
                throw new DOMException('Remove buffered media before reducing duration', 'InvalidStateError');
            this.__setDuration(value);
        }
        __setDuration(value) {
            const changed = this.__duration !== value;
            this.__duration = value;
            if (!this.__element) return;
            const state = mediaStateFor(this.__element);
            state.duration = value;
            state.seekable = new TimeRanges(timeRangesConstructionToken,
                Number.isFinite(value) && value > 0 ? [[0, value]] : []);
            if (changed) queueMediaEvent(this.__element, 'durationchange');
        }
        addSourceBuffer(type) {
            if (this.readyState !== 'open')
                throw new DOMException('The MediaSource is not open', 'InvalidStateError');
            if (!mediaSourceTypeSupported(type))
                throw new DOMException('The media type is not supported', 'NotSupportedError');
            const kind = mediaTrackKind(type);
            const existingKinds = [...this.sourceBuffers].map(buffer => mediaTrackKind(buffer.__type));
            if (existingKinds.length >= 2 || existingKinds.includes(kind)
                || (kind === 'muxed' && existingKinds.length)
                || existingKinds.includes('muxed'))
                throw new DOMException('The SourceBuffer configuration is not supported', 'QuotaExceededError');
            const buffer = new SourceBuffer(this, String(type));
            const items = [...this.sourceBuffers, buffer];
            this.sourceBuffers.__replace(items);
            this.activeSourceBuffers.__replace(items);
            queueMediaEvent(this.sourceBuffers, 'addsourcebuffer');
            return buffer;
        }
        removeSourceBuffer(buffer) {
            if (this.readyState === 'closed')
                throw new DOMException('The MediaSource is closed', 'InvalidStateError');
            const items = [...this.sourceBuffers];
            const index = items.indexOf(buffer);
            if (index < 0) throw new DOMException('SourceBuffer was not found', 'NotFoundError');
            if (buffer.updating) buffer.abort();
            this.__release(buffer.__bytes);
            buffer.__chunks = [];
            buffer.__bytes = 0;
            items.splice(index, 1);
            this.sourceBuffers.__replace(items);
            this.activeSourceBuffers.__replace(items);
            queueMediaEvent(this.sourceBuffers, 'removesourcebuffer');
        }
        endOfStream(error = undefined) {
            if (this.readyState !== 'open' || [...this.sourceBuffers].some(buffer => buffer.updating))
                throw new DOMException('The MediaSource cannot end now', 'InvalidStateError');
            if (error !== undefined && error !== 'network' && error !== 'decode')
                throw new TypeError('endOfStream error must be network or decode');
            if (error !== undefined) {
                this.__fail(error);
                return;
            }
            if (!this.__maybeCommit(true) && !this.__loadedState)
                throw new DOMException('A complete supported SourceBuffer configuration is required', 'NotSupportedError');
            this.readyState = 'ended';
            this.__setDuration(Math.max(0, ...[...this.sourceBuffers].flatMap(buffer =>
                buffer.__ranges.map(range => range[1]))));
            queueMediaEvent(this, 'sourceended');
        }
        setLiveSeekableRange() {
            throw new DOMException('Live MediaSource ranges are not supported', 'NotSupportedError');
        }
        clearLiveSeekableRange() {}
        __attach(element) {
            if (this.readyState !== 'closed')
                throw new DOMException('The MediaSource is already attached', 'InvalidStateError');
            this.__element = element;
            mediaSourceForElement.set(element, this);
            this.readyState = 'open';
            queueMediaEvent(this, 'sourceopen');
        }
        __reopen() {
            if (this.readyState !== 'ended') return;
            this.readyState = 'open';
            queueMediaEvent(this, 'sourceopen');
        }
        __loaded(duration, buffered) {
            this.__loadedState = true;
            this.__committing = false;
            this.__updateExtent(duration, buffered);
            this.__finishCommit();
            const playback = this.__pendingPlayback.splice(0);
            for (const pending of playback)
                mediaCommand(this.__element, pending.requestId, 'playback', true, pending.volumeMillis);
            this.__maybeCommit();
        }
        __appended(duration, buffered) {
            this.__committing = false;
            this.__updateExtent(duration, buffered);
            this.__finishCommit();
            if (this.__waiting && this.__element) {
                const state = mediaStateFor(this.__element);
                if (Number(duration) > state.currentTime) {
                    this.__waiting = false;
                    if (!state.paused)
                        mediaCommand(this.__element, 0, 'playback', true, effectiveVolumeMillis(state));
                }
            }
            this.__maybeCommit();
        }
        __updateExtent(duration, buffered) {
            const end = Math.max(0, Number(duration) || 0);
            // A decoded append extends buffered media, not an author's declared timeline.
            // MSE coded-frame processing may grow duration, but must never shrink it.
            this.__setDuration(Number.isNaN(this.__duration) ? end : Math.max(this.__duration, end));
            const tracks = buffered || [[0, end], [0, end]];
            for (const buffer of this.sourceBuffers) {
                const kind = mediaTrackKind(buffer.__type);
                const range = kind === 'video' ? tracks[0] : kind === 'audio' ? tracks[1]
                    : [Math.max(tracks[0][0], tracks[1][0]), Math.min(tracks[0][1], tracks[1][1])];
                if (range[1] > range[0]) buffer.__setBuffered(range[0], range[1]);
            }
        }
        __requestPlayback(requestId, volumeMillis) {
            if (this.__loadedState) {
                mediaCommand(this.__element, requestId, 'playback', true, volumeMillis);
                return;
            }
            this.__pendingPlayback.push({ requestId, volumeMillis });
            this.__maybeCommit();
        }
        __reserve(bytes) {
            const next = this.__encodedBytes + Number(bytes);
            if (!Number.isSafeInteger(next) || next > MAX_MEDIA_SOURCE_BYTES)
                throw new DOMException('The MediaSource encoded-byte budget was exceeded', 'QuotaExceededError');
            this.__encodedBytes = next;
        }
        __release(bytes) { this.__encodedBytes = Math.max(0, this.__encodedBytes - Number(bytes)); }
        __finishCommit(event = 'update') {
            for (const [buffer, operation] of this.__commitBuffers.splice(0))
                buffer.__finishUpdate(operation, event);
        }
        __maybeCommit(force = false) {
            if (this.__committing || !this.__element)
                return this.__committing;
            const populated = [...this.sourceBuffers].filter(buffer => buffer.__bytes > 0 && buffer.__hasMediaData);
            if (!populated.length) return false;
            const muxed = populated.length === 1 && mediaTrackKind(populated[0].__type) === 'muxed';
            const video = populated.find(buffer => mediaTrackKind(buffer.__type) === 'video');
            const audio = populated.find(buffer => mediaTrackKind(buffer.__type) === 'audio');
            if (!muxed && !this.__loadedState && !(video && audio)) return false;
            this.__committing = true;
            this.__commitBuffers = populated.map(buffer => [buffer, buffer.__operation]);
            if (muxed) {
                if (this.__loadedState) {
                    this.__committing = false;
                    return false;
                }
                const buffer = populated[0];
                const bytes = buffer.__takeBytes();
                try {
                    mediaCommand(this.__element, 0, 'commit', buffer.__type, bytes);
                } finally {
                    releaseInternalMediaBytes(bytes);
                }
            } else {
                const videoBytes = video ? video.__takeBytes() : new Uint8Array();
                const audioBytes = audio ? audio.__takeBytes() : new Uint8Array();
                const videoType = [...this.sourceBuffers].find(buffer => mediaTrackKind(buffer.__type) === 'video')?.__type || '';
                const audioType = [...this.sourceBuffers].find(buffer => mediaTrackKind(buffer.__type) === 'audio')?.__type || '';
                try {
                    mediaCommand(this.__element, 0,
                        this.__loadedState ? 'append-adaptive' : 'commit-adaptive',
                        videoType, videoBytes, audioType, audioBytes);
                } finally {
                    releaseInternalMediaBytes(videoBytes);
                    releaseInternalMediaBytes(audioBytes);
                }
            }
            return true;
        }
        __bufferedChanged() {
            if (!this.__element) return;
            // MSE exposes the intersection of active track buffers, never their union/max end.
            let ranges = null;
            for (const buffer of this.activeSourceBuffers) {
                const next = buffer.__ranges;
                ranges = ranges === null ? next.map(range => [...range])
                    : ranges.flatMap(([start, end]) => next.flatMap(([otherStart, otherEnd]) => {
                        const low = Math.max(start, otherStart), high = Math.min(end, otherEnd);
                        return high > low ? [[low, high]] : [];
                    }));
            }
            mediaStateFor(this.__element).buffered =
                new TimeRanges(timeRangesConstructionToken, ranges || []);
        }
        __fail(kind) {
            if (this.__element && mediaStateFor(this.__element).error) return;
            this.__finishCommit('error');
            host('console', 'error', 'MediaSource failed: ' + String(kind));
            this.readyState = 'ended';
            const element = this.__element;
            if (element) {
                const state = mediaStateFor(element);
                state.error = new MediaError(
                    kind === 'network' ? MediaError.MEDIA_ERR_NETWORK : MediaError.MEDIA_ERR_DECODE,
                    'MediaSource ' + kind + ' failure'
                );
                state.networkState = HTMLMediaElement.NETWORK_IDLE;
                queueMediaEvent(element, 'error');
            }
            for (const pending of this.__pendingPlayback.splice(0)) {
                const request = pendingMediaRequests.get(pending.requestId);
                pendingMediaRequests.delete(pending.requestId);
                request?.reject(new DOMException('MediaSource decode failed', 'NotSupportedError'));
            }
            queueMediaEvent(this, 'sourceended');
        }
    }
    installEventHandlerAttributes(MediaSource.prototype);

    const notifyMediaSourceLoaded = (element, duration, buffered) =>
        mediaSourceForElement.get(element)?.__loaded(duration, buffered);
    const notifyMediaSourceAppended = (element, duration, buffered) =>
        mediaSourceForElement.get(element)?.__appended(duration, buffered);
    const notifyMediaSourceError = element =>
        mediaSourceForElement.get(element)?.__fail('decode');
    const waitForMediaSourceData = (element, position) => {
        const source = mediaSourceForElement.get(element);
        if (!source || source.readyState === 'ended') return false;
        const state = mediaStateFor(element);
        state.currentTime = Math.max(0, Number(position) || 0);
        state.readyState = HTMLMediaElement.HAVE_CURRENT_DATA;
        if (!source.__waiting) queueMediaEvent(element, 'waiting');
        source.__waiting = true;
        return true;
    };
    const prepareMediaSourcePlayback = (element, requestId, volumeMillis) => {
        const source = mediaSourceForElement.get(element);
        if (!source) return false;
        source.__requestPlayback(requestId, volumeMillis);
        return true;
    };

    Object.defineProperty(HTMLMediaElement.prototype, 'src', {
        configurable: true,
        get() {
            const value = this.getAttribute('src');
            if (value == null) return '';
            return objectUrlEntries.has(value) ? value : host('resolveUrl', value);
        },
        set(value) {
            value = String(value);
            this.setAttribute('src', value);
            const object = objectUrlValue(value);
            if (object instanceof MediaSource) {
                const state = mediaStateFor(this);
                state.networkState = HTMLMediaElement.NETWORK_LOADING;
                state.currentSrc = value;
                this.dispatchEvent(new Event('loadstart'));
                object.__attach(this);
            }
        }
    });
