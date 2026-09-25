(() => {
    'use strict';
    const typeError = message => Promise.reject(new TypeError(message));
    let byteBindings;
    Object.defineProperty(globalThis, '__installReadableByteStreams', {
        configurable: true, value(bindings) { byteBindings = bindings; }
    });

    class ReadableStreamDefaultController {
        constructor(stream) { this.__stream = stream; }
        get desiredSize() {
            const stream = this.__stream;
            if (stream.__state === 'errored') return null;
            return stream.__state === 'closed' ? 0 : stream.__highWaterMark - stream.__queueSize;
        }
        enqueue(chunk) {
            const stream = this.__stream;
            if (stream.__state !== 'readable' || stream.__closeRequested)
                throw new TypeError('ReadableStream is not in a state that permits enqueue');
            const pending = stream.__reads.shift();
            if (pending) pending.resolve({ value: chunk, done: false });
            else {
                let size;
                try {
                    size = Number(stream.__size(chunk));
                    if (!Number.isFinite(size) || size < 0) throw new RangeError('Invalid chunk size');
                } catch (error) { stream.__finishError(error); throw error; }
                // A strategy callback can close or error the stream reentrantly.
                // The chunk must not resurrect a closed queue or hide the error.
                if (stream.__state !== 'readable') return;
                stream.__queue.push({ value: chunk, size });
                stream.__queueSize += size;
            }
            stream.__pullIfNeeded();
        }
        close() {
            const stream = this.__stream;
            if (stream.__state !== 'readable' || stream.__closeRequested)
                throw new TypeError('ReadableStream is not in a state that permits close');
            stream.__closeRequested = true;
            if (!stream.__queue.length) stream.__finishClose();
        }
        error(reason) { this.__stream.__finishError(reason); }
    }

    class ReadableStreamDefaultReader {
        constructor(stream) {
            if (!(stream instanceof ReadableStream) || stream.locked)
                throw new TypeError('Reader requires an unlocked ReadableStream');
            this.__stream = stream;
            stream.__reader = this;
        }
        get closed() {
            return this.__stream ? this.__stream.__closedPromise
                : this.__releasedClosed;
        }
        read() {
            return new Promise((resolve, reject) => this.__readInternal(
                value => resolve({ value, done: false }),
                () => resolve({ value: undefined, done: true }), reject));
        }
        // Internal consumers receive callbacks, not an author-observable
        // iterator-result promise. A polluted Object.prototype.then must not
        // intercept stream piping or tee's private read requests.
        __readInternal(onChunk, onClose, onError) {
            const stream = this.__stream;
            if (!stream) { onError(new TypeError('Reader was released')); return; }
            stream.__disturb();
            if (stream.__queue.length) {
                const { value, size } = stream.__queue.shift();
                stream.__queueSize = Math.max(0, stream.__queueSize - size);
                if (stream.__closeRequested && !stream.__queue.length) stream.__finishClose();
                else stream.__pullIfNeeded();
                onChunk(value); return;
            }
            if (stream.__state === 'closed') { onClose(); return; }
            if (stream.__state === 'errored') { onError(stream.__storedError); return; }
            const request = {
                resolve(result) { if (result.done) onClose(); else onChunk(result.value); },
                reject: onError
            };
            stream.__reads.push(request);
            stream.__pullIfNeeded();
        }
        cancel(reason) {
            return this.__stream ? this.__stream.__cancel(reason) : typeError('Reader was released');
        }
        releaseLock() {
            const stream = this.__stream;
            if (!stream) return;
            const error = new TypeError('Reader was released');
            for (const pending of stream.__reads.splice(0)) pending.reject(error);
            if (stream.__byteStream) byteBindings.releaseDefaultReader(stream);
            if (stream.__state === 'readable') stream.__resetClosedPromise(error);
            this.__releasedClosed = Promise.reject(error);
            this.__releasedClosed.catch(() => {});
            stream.__reader = null; this.__stream = null;
        }
    }

    class ReadableStream {
        constructor(source = {}, strategy = {}) {
            if (source === null) throw new TypeError('ReadableStream source must not be null');
            source = Object(source); strategy = Object(strategy);
            const sourceType = source.type;
            const type = sourceType === undefined ? undefined : String(sourceType);
            if (type !== undefined && type !== 'bytes')
                throw new TypeError('Unsupported ReadableStream source type');
            this.__byteStream = type === 'bytes';
            if (this.__byteStream && strategy.size !== undefined)
                throw new RangeError('Byte stream strategy cannot specify size');
            this.__state = 'readable'; this.__storedError = undefined;
            this.__queue = []; this.__reads = []; this.__reader = null;
            this.__byobReads = [];
            this.__queueSize = 0;
            this.__size = this.__byteStream ? chunk => chunk.byteLength
                : strategy.size === undefined ? (() => 1) : strategy.size;
            if (typeof this.__size !== 'function') throw new TypeError('size must be callable');
            this.__disturbed = false; this.__closeRequested = false;
            this.__started = false; this.__pulling = false; this.__pullAgain = false;
            this.__source = source;
            this.__pull = source.pull;
            if (this.__pull !== undefined && typeof this.__pull !== 'function')
                throw new TypeError('pull must be callable');
            this.__sourceCancel = source.cancel;
            if (this.__sourceCancel !== undefined && typeof this.__sourceCancel !== 'function')
                throw new TypeError('cancel must be callable');
            this.__highWaterMark = strategy.highWaterMark === undefined
                ? (this.__byteStream ? 0 : 1) : Number(strategy.highWaterMark);
            if (Number.isNaN(this.__highWaterMark) || this.__highWaterMark < 0)
                throw new RangeError('highWaterMark must be non-negative');
            this.__closedPromise = new Promise((resolve, reject) => {
                this.__closeResolve = resolve; this.__closeReject = reject;
            });
            this.__closedPromise.catch(() => {});
            this.__controller = this.__byteStream
                ? byteBindings.createController(this, source) : new ReadableStreamDefaultController(this);
            const started = source.start?.(this.__controller);
            Promise.resolve(started).then(() => {
                this.__started = true; this.__pullIfNeeded();
            }, error => this.__finishError(error));
        }
        get locked() { return this.__reader !== null; }
        getReader(options = undefined) {
            if (this.locked) throw new TypeError('ReadableStream is locked');
            const mode = options?.mode === undefined ? undefined : String(options.mode);
            if (mode !== undefined && mode !== 'byob')
                throw new TypeError('Unsupported ReadableStream reader mode');
            if (mode === 'byob' && !this.__byteStream)
                throw new TypeError('BYOB readers require a readable byte stream');
            const reader = mode === 'byob'
                ? new byteBindings.Reader(this) : new ReadableStreamDefaultReader(this);
            if (mode === 'byob') this.__reader = reader;
            return reader;
        }
        __resetClosedPromise(reason) {
            this.__closeReject(reason);
            this.__closedPromise = new Promise((resolve, reject) => {
                this.__closeResolve = resolve; this.__closeReject = reject;
            });
            this.__closedPromise.catch(() => {});
        }
        cancel(reason) {
            if (this.locked) return typeError('Cannot cancel a locked ReadableStream');
            return this.__cancel(reason);
        }
        __disturb() {
            if (!this.__disturbed) { this.__disturbed = true; this.__onDisturb?.(); }
        }
        __cancel(reason) {
            this.__disturb(); this.__queue.length = 0; this.__queueSize = 0;
            if (this.__state === 'closed') return Promise.resolve();
            if (this.__state === 'errored') return Promise.reject(this.__storedError);
            if (this.__byteStream) byteBindings.cancelReads(this);
            this.__finishClose();
            try { return Promise.resolve(this.__sourceCancel?.call(this.__source, reason)).then(() => undefined); }
            catch (error) { return Promise.reject(error); }
        }
        __finishClose() {
            if (this.__state !== 'readable') return;
            this.__state = 'closed';
            for (const pending of this.__reads.splice(0))
                pending.resolve({ value: undefined, done: true });
            if (this.__byteStream) byteBindings.closeReads(this);
            this.__closeResolve();
        }
        __finishError(reason) {
            if (this.__state !== 'readable') return;
            this.__state = 'errored'; this.__storedError = reason; this.__queue.length = 0; this.__queueSize = 0;
            this.__closeReject(reason);
            for (const pending of this.__reads.splice(0)) pending.reject(reason);
            if (this.__byteStream) byteBindings.errorReads(this, reason);
            this.__errorObservers?.forEach(observer => observer(reason));
        }
        __pullIfNeeded() {
            if (!this.__started || this.__state !== 'readable' || this.__closeRequested ||
                typeof this.__pull !== 'function' ||
                (!this.__reads.length && !this.__byobReads.length &&
                    this.__controller.desiredSize <= 0)) return;
            if (this.__pulling) { this.__pullAgain = true; return; }
            this.__pulling = true;
            let result;
            try { result = this.__pull.call(this.__source, this.__controller); }
            // The underlying pull algorithm is promise-returning, including thrown errors.
            // Preserve its microtask ordering relative to an already fulfilled read.
            catch (error) { result = Promise.reject(error); }
            Promise.resolve(result).then(() => {
                this.__pulling = false;
                if (this.__pullAgain) { this.__pullAgain = false; this.__pullIfNeeded(); }
            }, error => { this.__pulling = false; this.__finishError(error); });
        }
        __tee(cloneSecondBranch) {
            if (this.locked) throw new TypeError('ReadableStream is locked');
            const reader = this.getReader(); const branches = [{}, {}];
            let reading = false, readAgain = false, resolveCancel;
            const cancellation = new Promise(resolve => { resolveCancel = resolve; });
            const fail = error => {
                for (const branch of branches) if (!branch.cancelled) branch.controller.error(error);
                if (branches.some(branch => !branch.cancelled)) resolveCancel();
            };
            // Streams' default tee is demand driven. A slow branch may retain data requested
            // by its faster peer, but two idle branches must not drain the transport eagerly.
            const pull = () => {
                if (reading) { readAgain = true; return; }
                reading = true;
                reader.__readInternal(value => {
                    // The tee chunk steps run in a microtask so an already
                    // queued reader.closed rejection can error both branches
                    // before a synchronous read enqueues its chunk.
                    queueMicrotask(() => {
                        if (branches.some(branch => !branch.cancelled &&
                            branch.controller.__stream.__state === 'errored')) {
                            reading = false; return;
                        }
                        readAgain = false;
                        let secondValue = value;
                        if (cloneSecondBranch && !branches[1].cancelled) {
                            try { secondValue = structuredClone(value); }
                            catch (error) {
                                for (const branch of branches) if (!branch.cancelled) branch.controller.error(error);
                                resolveCancel(reader.cancel(error)); return;
                            }
                        }
                        if (!branches[0].cancelled) branches[0].controller.enqueue(value);
                        if (!branches[1].cancelled) branches[1].controller.enqueue(secondValue);
                        reading = false;
                        if (readAgain) pull();
                    });
                }, () => {
                    reading = false;
                    for (const branch of branches) if (!branch.cancelled) branch.controller.close();
                    if (branches.some(branch => !branch.cancelled)) resolveCancel();
                }, error => { reading = false; fail(error); });
            };
            const streams = branches.map(branch => new ReadableStream({
                ...(this.__byteStream ? { type: 'bytes' } : {}),
                start(controller) { branch.controller = controller; },
                pull,
                cancel(reason) {
                    branch.cancelled = true; branch.reason = reason;
                    if (branches.every(item => item.cancelled))
                        resolveCancel(reader.cancel(branches.map(item => item.reason)));
                    return cancellation;
                }
            }));
            reader.closed.catch(fail);
            return streams;
        }
        tee() { return this.__byteStream ? byteBindings.tee(this) : this.__tee(false); }
    }

    Object.assign(globalThis, {
        ReadableStream, ReadableStreamDefaultReader, ReadableStreamDefaultController
    });
})();
