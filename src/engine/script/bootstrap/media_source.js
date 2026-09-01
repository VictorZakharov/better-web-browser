    // A deliberately bounded Media Source implementation. It supports the two SourceBuffer
    // configurations required by MSE: one muxed buffer, or separate video and audio buffers.
    // Encoded tracks cross the contained-media boundary once when playback is requested.
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
    const mediaTrackKind = type => {
        const source = String(type).toLowerCase();
        const video = source.includes('avc1.');
        const audio = source.includes('mp4a.40.2');
        if (video && audio) return 'muxed';
        if (video) return 'video';
        if (audio) return 'audio';
        return '';
    };
    const completeMediaDataBoxes = bytes => {
        const read32 = offset => ((bytes[offset] << 24) | (bytes[offset + 1] << 16)
            | (bytes[offset + 2] << 8) | bytes[offset + 3]) >>> 0;
        let offset = 0;
        let foundMediaData = false;
        while (offset + 8 <= bytes.byteLength) {
            let size = read32(offset);
            let header = 8;
            if (size === 1) {
                if (offset + 16 > bytes.byteLength) return false;
                const high = read32(offset + 8);
                const low = read32(offset + 12);
                size = high * 0x100000000 + low;
                header = 16;
                if (!Number.isSafeInteger(size)) return false;
            } else if (size === 0) {
                size = bytes.byteLength - offset;
            }
            if (size < header || offset + size > bytes.byteLength) return false;
            foundMediaData ||= bytes[offset + 4] === 0x6d && bytes[offset + 5] === 0x64
                && bytes[offset + 6] === 0x61 && bytes[offset + 7] === 0x74;
            offset += size;
        }
        return foundMediaData && offset === bytes.byteLength;
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

    class SourceBufferList extends EventTarget {
        constructor() {
            super();
            this.__items = [];
        }
        get length() { return this.__items.length; }
        item(index) { return this.__items[Number(index)] || null; }
        [Symbol.iterator]() { return this.__items[Symbol.iterator](); }
        __replace(items) {
            for (let index = 0; index < this.__items.length; index++) delete this[index];
            this.__items = [...items];
            for (let index = 0; index < this.__items.length; index++)
                Object.defineProperty(this, index, { configurable: true, get: () => this.__items[index] });
        }
    }

    class SourceBuffer extends EventTarget {
        constructor(parent, type) {
            super();
            this.__parent = parent;
            this.__type = type;
            this.__chunks = [];
            this.__bytes = 0;
            this.__reservedBytes = 0;
            this.__hasMediaData = false;
            this.__ranges = [];
            this.__operation = 0;
            this.updating = false;
            this.mode = 'segments';
            this.timestampOffset = 0;
            this.appendWindowStart = 0;
            this.appendWindowEnd = Infinity;
        }
        get buffered() { return new TimeRanges(timeRangesConstructionToken, this.__ranges); }
        appendBuffer(value) {
            this.__prepareUpdate();
            if (this.updating) throw new DOMException('The SourceBuffer is updating', 'InvalidStateError');
            const bytes = copyMediaBytes(value);
            if (!bytes) throw new TypeError('appendBuffer requires an ArrayBuffer or view');
            this.__parent.__reserve(bytes.byteLength);
            this.__reservedBytes += bytes.byteLength;
            this.__beginUpdate(() => {
                this.__chunks.push(bytes);
                this.__bytes += bytes.byteLength;
                this.__reservedBytes -= bytes.byteLength;
                this.__hasMediaData = completeMediaDataBoxes(this.__materialize());
            });
        }
        abort() {
            this.__requireOpen();
            if (!this.updating) return;
            this.__operation++;
            this.updating = false;
            this.__parent.__release(this.__reservedBytes);
            this.__reservedBytes = 0;
            queueMediaEvent(this, 'abort');
            queueMediaEvent(this, 'updateend');
        }
        remove(start, end) {
            if (this.updating) throw new DOMException('The SourceBuffer is updating', 'InvalidStateError');
            start = Number(start);
            end = Number(end);
            const duration = Number(this.__parent.duration);
            if (!Number.isFinite(duration) || !Number.isFinite(start) || start < 0
                || start > duration || Number.isNaN(end) || end <= start)
                throw new TypeError('remove requires an increasing finite time range');
            this.__prepareUpdate();
            this.__beginUpdate(() => {
                this.__ranges = this.__ranges.flatMap(([rangeStart, rangeEnd]) => {
                    if (end <= rangeStart || start >= rangeEnd) return [[rangeStart, rangeEnd]];
                    const ranges = [];
                    if (start > rangeStart) ranges.push([rangeStart, Math.min(start, rangeEnd)]);
                    if (end < rangeEnd) ranges.push([Math.max(end, rangeStart), rangeEnd]);
                    return ranges;
                });
                if (!this.__ranges.length) {
                    this.__parent.__release(this.__bytes);
                    this.__chunks = [];
                    this.__bytes = 0;
                    this.__hasMediaData = false;
                }
                this.__parent.__bufferedChanged();
            });
        }
        changeType(type) {
            this.__requireOpen();
            if (this.updating) throw new DOMException('The SourceBuffer is updating', 'InvalidStateError');
            if (!mediaSourceTypeSupported(type))
                throw new DOMException('The media type is not supported', 'NotSupportedError');
            this.__type = String(type);
        }
        __beginUpdate(apply) {
            this.updating = true;
            const operation = ++this.__operation;
            queueMicrotask(() => {
                if (operation !== this.__operation || !this.updating) return;
                this.dispatchEvent(markTrusted(new Event('updatestart')));
                try {
                    apply();
                    this.updating = false;
                    this.dispatchEvent(markTrusted(new Event('update')));
                } catch (_error) {
                    this.__parent.__release(this.__reservedBytes);
                    this.__reservedBytes = 0;
                    this.updating = false;
                    this.dispatchEvent(markTrusted(new Event('error')));
                }
                this.dispatchEvent(markTrusted(new Event('updateend')));
                this.__parent.__maybeCommit();
            });
        }
        __requireOpen() {
            if (this.__parent.readyState !== 'open')
                throw new DOMException('The MediaSource is not open', 'InvalidStateError');
        }
        __prepareUpdate() {
            if (this.__parent.readyState === 'closed')
                throw new DOMException('The MediaSource is closed', 'InvalidStateError');
            if (this.__parent.readyState === 'ended') this.__parent.__reopen();
        }
        __materialize() { return concatMediaBytes(this.__chunks); }
        __takeBytes() {
            const bytes = this.__materialize();
            this.__parent.__release(this.__bytes);
            this.__chunks = [];
            this.__bytes = 0;
            return bytes;
        }
        __setBuffered(duration) {
            const start = Math.max(0, this.appendWindowStart);
            const end = Math.min(Number(duration) || 0, this.appendWindowEnd);
            this.__ranges = end > start ? [[start, end]] : [];
            this.__parent.__bufferedChanged();
        }
    }
    installEventHandlerAttributes(SourceBuffer.prototype);

    class MediaSource extends EventTarget {
        constructor() {
            super();
            this.readyState = 'closed';
            this.duration = NaN;
            this.sourceBuffers = new SourceBufferList();
            this.activeSourceBuffers = new SourceBufferList();
            this.__element = null;
            this.__encodedBytes = 0;
            this.__committing = false;
            this.__loadedState = false;
            this.__pendingPlayback = [];
        }
        static isTypeSupported(type) { return mediaSourceTypeSupported(type); }
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
            if (!this.__maybeCommit(true))
                throw new DOMException('A complete supported SourceBuffer configuration is required', 'NotSupportedError');
            this.readyState = 'ended';
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
        __loaded(duration) {
            this.__loadedState = true;
            this.duration = Number(duration);
            for (const buffer of this.sourceBuffers) buffer.__setBuffered(this.duration);
            const playback = this.__pendingPlayback.splice(0);
            for (const pending of playback)
                mediaCommand(this.__element, pending.requestId, 'playback', true, pending.volumeMillis);
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
        __maybeCommit(force = false) {
            if (this.__committing || !this.__element || [...this.sourceBuffers].some(buffer => buffer.updating))
                return this.__committing;
            if (!force && !this.__pendingPlayback.length) return false;
            const populated = [...this.sourceBuffers].filter(buffer => buffer.__bytes > 0);
            if (!populated.length || populated.some(buffer => !buffer.__hasMediaData)) return false;
            const muxed = populated.length === 1 && mediaTrackKind(populated[0].__type) === 'muxed';
            const video = populated.find(buffer => mediaTrackKind(buffer.__type) === 'video');
            const audio = populated.find(buffer => mediaTrackKind(buffer.__type) === 'audio');
            if (!muxed && !(populated.length === 2 && video && audio)) return false;
            this.__committing = true;
            if (muxed) {
                const buffer = populated[0];
                mediaCommand(this.__element, 0, 'commit', buffer.__type, buffer.__takeBytes());
            } else {
                mediaCommand(this.__element, 0, 'commit-adaptive',
                    video.__type, video.__takeBytes(), audio.__type, audio.__takeBytes());
            }
            return true;
        }
        __bufferedChanged() {
            if (!this.__element) return;
            const buffer = this.sourceBuffers.item(0);
            const ranges = buffer ? buffer.__ranges.map(range => [...range]) : [];
            mediaStateFor(this.__element).buffered =
                new TimeRanges(timeRangesConstructionToken, ranges);
        }
        __fail(kind) {
            this.readyState = 'ended';
            const element = this.__element;
            if (element) {
                const state = mediaStateFor(element);
                state.error = new MediaError(
                    kind === 'network' ? MediaError.MEDIA_ERR_NETWORK : MediaError.MEDIA_ERR_DECODE,
                    'MediaSource ' + kind + ' failure'
                );
                element.dispatchEvent(new Event('error'));
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

    const notifyMediaSourceLoaded = (element, duration) =>
        mediaSourceForElement.get(element)?.__loaded(duration);
    const notifyMediaSourceError = element =>
        mediaSourceForElement.get(element)?.__fail('decode');
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
