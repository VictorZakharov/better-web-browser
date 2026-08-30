    // A deliberately bounded Media Source implementation. Encoded bytes remain renderer-owned
    // until endOfStream(), then cross the existing contained-media boundary as one admitted source.
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

    const mediaSourceTypeSupported = type => {
        const source = String(type).toLowerCase();
        return supportedMediaType(source) === 'probably'
            && source.includes('avc1.')
            && source.includes('mp4a.40.2');
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
            this.__requireOpen();
            if (this.updating) throw new DOMException('The SourceBuffer is updating', 'InvalidStateError');
            const bytes = copyMediaBytes(value);
            if (!bytes) throw new TypeError('appendBuffer requires an ArrayBuffer or view');
            const nextSize = this.__bytes + bytes.byteLength;
            if (nextSize > MAX_MEDIA_SOURCE_BYTES)
                throw new DOMException('The MediaSource encoded-byte budget was exceeded', 'QuotaExceededError');
            this.__beginUpdate(() => {
                this.__chunks.push(bytes);
                this.__bytes = nextSize;
            });
        }
        abort() {
            this.__requireOpen();
            if (!this.updating) return;
            this.__operation++;
            this.updating = false;
            this.dispatchEvent(new Event('abort'));
            this.dispatchEvent(new Event('updateend'));
        }
        remove(start, end) {
            this.__requireOpen();
            if (this.updating) throw new DOMException('The SourceBuffer is updating', 'InvalidStateError');
            start = Number(start);
            end = Number(end);
            if (!Number.isFinite(start) || start < 0 || !Number.isFinite(end) || end <= start)
                throw new TypeError('remove requires an increasing finite time range');
            this.__beginUpdate(() => {
                this.__ranges = this.__ranges.flatMap(([rangeStart, rangeEnd]) => {
                    if (end <= rangeStart || start >= rangeEnd) return [[rangeStart, rangeEnd]];
                    const ranges = [];
                    if (start > rangeStart) ranges.push([rangeStart, Math.min(start, rangeEnd)]);
                    if (end < rangeEnd) ranges.push([Math.max(end, rangeStart), rangeEnd]);
                    return ranges;
                });
                if (!this.__ranges.length) {
                    this.__chunks = [];
                    this.__bytes = 0;
                }
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
            ++this.__operation;
            this.dispatchEvent(new Event('updatestart'));
            try {
                apply();
                this.dispatchEvent(new Event('update'));
            } finally {
                this.updating = false;
                this.dispatchEvent(new Event('updateend'));
            }
        }
        __requireOpen() {
            if (this.__parent.readyState !== 'open')
                throw new DOMException('The MediaSource is not open', 'InvalidStateError');
        }
        __materialize() { return concatMediaBytes(this.__chunks); }
        __setBuffered(duration) {
            const start = Math.max(0, this.appendWindowStart);
            const end = Math.min(Number(duration) || 0, this.appendWindowEnd);
            this.__ranges = end > start ? [[start, end]] : [];
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
        }
        static isTypeSupported(type) { return mediaSourceTypeSupported(type); }
        addSourceBuffer(type) {
            if (this.readyState !== 'open')
                throw new DOMException('The MediaSource is not open', 'InvalidStateError');
            if (!mediaSourceTypeSupported(type))
                throw new DOMException('The media type is not supported', 'NotSupportedError');
            const buffer = new SourceBuffer(this, String(type));
            const items = [...this.sourceBuffers, buffer];
            this.sourceBuffers.__replace(items);
            this.activeSourceBuffers.__replace(items);
            this.sourceBuffers.dispatchEvent(new Event('addsourcebuffer'));
            return buffer;
        }
        removeSourceBuffer(buffer) {
            if (this.readyState === 'closed')
                throw new DOMException('The MediaSource is closed', 'InvalidStateError');
            const items = [...this.sourceBuffers];
            const index = items.indexOf(buffer);
            if (index < 0) throw new DOMException('SourceBuffer was not found', 'NotFoundError');
            items.splice(index, 1);
            this.sourceBuffers.__replace(items);
            this.activeSourceBuffers.__replace(items);
            this.sourceBuffers.dispatchEvent(new Event('removesourcebuffer'));
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
            const populated = [...this.sourceBuffers].filter(buffer => buffer.__bytes > 0);
            if (populated.length !== 1 || !this.__element)
                throw new DOMException('Exactly one muxed SourceBuffer is required', 'NotSupportedError');
            const buffer = populated[0];
            mediaCommand(this.__element, 0, 'commit', buffer.__type, buffer.__materialize());
            this.readyState = 'ended';
            this.dispatchEvent(new Event('sourceended'));
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
            this.dispatchEvent(new Event('sourceopen'));
        }
        __loaded(duration) {
            this.duration = Number(duration);
            for (const buffer of this.sourceBuffers) buffer.__setBuffered(this.duration);
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
            this.dispatchEvent(new Event('sourceended'));
        }
    }
    installEventHandlerAttributes(MediaSource.prototype);

    const notifyMediaSourceLoaded = (element, duration) =>
        mediaSourceForElement.get(element)?.__loaded(duration);
    const notifyMediaSourceError = element =>
        mediaSourceForElement.get(element)?.__fail('decode');

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
