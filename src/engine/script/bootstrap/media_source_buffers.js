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
            this.__completeBytes = 0;
            this.__initializationLength = 0;
            this.__initializationBytes = null;
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
                const parsed = completeMediaSegmentPrefix(this.__materialize());
                if (parsed.invalid) throw new DOMException(
                    'The appended ISO-BMFF byte stream is invalid',
                    'InvalidStateError'
                );
                this.__completeBytes = parsed.length;
                this.__initializationLength = parsed.initializationLength;
                this.__hasMediaData = parsed.hasMediaData;
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
                } catch (error) {
                    host('console', 'error', 'SourceBuffer update failed: ' + String(error));
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
            const materialized = this.__materialize();
            const ready = materialized.slice(0, this.__completeBytes);
            const pending = materialized.slice(this.__completeBytes);
            if (!this.__initializationBytes && this.__initializationLength > 0)
                this.__initializationBytes = ready.slice(0, this.__initializationLength);
            const transfer = this.__initializationBytes && this.__initializationLength === 0
                ? concatMediaBytes([this.__initializationBytes, ready])
                : ready;
            this.__parent.__release(ready.byteLength);
            this.__chunks = pending.byteLength ? [pending] : [];
            this.__bytes = pending.byteLength;
            this.__completeBytes = 0;
            this.__initializationLength = 0;
            this.__hasMediaData = false;
            return transfer;
        }
        __setBuffered(duration) {
            const start = Math.max(0, this.appendWindowStart);
            const end = Math.min(Number(duration) || 0, this.appendWindowEnd);
            this.__ranges = end > start ? [[start, end]] : [];
            this.__parent.__bufferedChanged();
        }
    }
    installEventHandlerAttributes(SourceBuffer.prototype);
