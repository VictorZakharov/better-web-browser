(() => {
    'use strict';
    const reject = message => Promise.reject(new TypeError(message));
    const controllerToken = Symbol('WritableStreamDefaultController');

    class WritableStreamDefaultController {
        constructor(stream, token) {
            if (token !== controllerToken)
                throw new TypeError('WritableStreamDefaultController is not constructible');
            this.__stream = stream;
        }
        get signal() { return this.__stream.__abortController.signal; }
        error(reason) { this.__stream.__error(reason); }
    }

    class WritableStreamDefaultWriter {
        constructor(stream) {
            if (!(stream instanceof WritableStream) || stream.locked)
                throw new TypeError('Writer requires an unlocked WritableStream');
            this.__stream = stream;
            stream.__writer = this;
        }
        get closed() { return this.__stream ? this.__stream.__closedPromise : this.__releasedClosed; }
        get ready() { return this.__stream ? this.__stream.__ready : this.__releasedReady; }
        get desiredSize() {
            if (!this.__stream) throw new TypeError('Writer was released');
            if (this.__stream.__state === 'errored' || this.__stream.__state === 'erroring') return null;
            if (this.__stream.__state === 'closed') return 0;
            return this.__stream.__highWaterMark - this.__stream.__queueSize;
        }
        write(chunk) { return this.__stream ? this.__stream.__write(chunk, this) : reject('Writer was released'); }
        close() { return this.__stream ? this.__stream.__close() : reject('Writer was released'); }
        abort(reason) { return this.__stream ? this.__stream.__abort(reason) : reject('Writer was released'); }
        releaseLock() {
            const stream = this.__stream;
            if (!stream) return;
            const error = new TypeError('Writer was released');
            if (stream.__readyReject) stream.__resetReadyPromise(error);
            if (stream.__state === 'writable' || stream.__state === 'closing' ||
                stream.__state === 'erroring')
                stream.__resetClosedPromise(error);
            this.__releasedReady = Promise.reject(error);
            if (stream.__abortSettling) {
                // The original closed promise is still pending while an async
                // underlying abort runs; releasing rejects that same promise.
                stream.__closeReject(error);
                this.__releasedClosed = stream.__closedPromise;
            } else this.__releasedClosed = Promise.reject(error);
            this.__releasedReady.catch(() => {});
            this.__releasedClosed.catch(() => {});
            stream.__writer = null;
            this.__stream = null;
        }
    }

    class WritableStream {
        constructor(sink = {}, strategy = {}) {
            sink = Object(sink); strategy = Object(strategy);
            // The queuing strategy is converted before the underlying sink.
            // Observable getters must run in that order.
            this.__size = strategy.size === undefined ? (() => 1) : strategy.size;
            if (typeof this.__size !== 'function') throw new TypeError('size must be callable');
            this.__highWaterMark = strategy.highWaterMark === undefined ? 1 : Number(strategy.highWaterMark);
            if (Number.isNaN(this.__highWaterMark) || this.__highWaterMark < 0)
                throw new RangeError('highWaterMark must be non-negative');
            if (sink.type !== undefined) throw new RangeError('Unsupported WritableStream sink type');
            this.__sink = sink;
            // Web IDL converts callback members during construction, not on the
            // first write/close/abort. Capture each callback and its this value.
            this.__sinkStart = sink.start;
            this.__sinkWrite = sink.write;
            this.__sinkClose = sink.close;
            this.__sinkAbort = sink.abort;
            for (const callback of [this.__sinkStart, this.__sinkWrite,
                this.__sinkClose, this.__sinkAbort]) {
                if (callback !== undefined && typeof callback !== 'function')
                    throw new TypeError('Underlying sink callback must be callable');
            }
            this.__state = 'writable';
            this.__storedError = undefined;
            this.__writer = null;
            this.__controller = new WritableStreamDefaultController(this, controllerToken);
            this.__queue = [];
            this.__queueSize = 0;
            this.__processing = null;
            this.__started = false;
            this.__abortController = new AbortController();
            this.__pendingAbort = null;
            this.__closeAbort = null;
            this.__erroringCloseRequests = [];
            this.__abortSettling = false;
            this.__aborted = false;
            this.__ready = Promise.resolve();
            this.__readyResolve = null;
            this.__readyReject = null;
            this.__closedPromise = new Promise((resolve, reject) => {
                this.__closeResolve = resolve;
                this.__closeReject = reject;
            });
            // A stream can error before an author acquires a writer. Keep the stored promise
            // observable while avoiding an unrelated unhandled-rejection report.
            this.__closedPromise.catch(() => {});
            this.__updateBackpressure();
            let started;
            started = this.__sinkStart === undefined ? undefined
                : Reflect.apply(this.__sinkStart, sink, [this.__controller]);
            Promise.resolve(started).then(() => {
                this.__started = true;
                this.__drain();
            }, error => {
                this.__started = true;
                this.__error(error);
                this.__drain();
            });
        }
        get locked() { return this.__writer !== null; }
        getWriter() {
            return new WritableStreamDefaultWriter(this);
        }
        __resetClosedPromise(reason) {
            this.__closeReject(reason);
            this.__closedPromise = new Promise((resolve, reject) => {
                this.__closeResolve = resolve;
                this.__closeReject = reject;
            });
            this.__closedPromise.catch(() => {});
        }
        __resetReadyPromise(reason) {
            this.__readyReject?.(reason);
            this.__readyResolve = this.__readyReject = null;
            if (this.__state === 'writable' && this.__queueSize >= this.__highWaterMark) {
                this.__ready = new Promise((resolve, reject) => {
                    this.__readyResolve = resolve;
                    this.__readyReject = reject;
                });
                this.__ready.catch(() => {});
            } else this.__ready = Promise.resolve();
        }
        __updateBackpressure() {
            if (this.__state !== 'writable') return;
            if (this.__queueSize >= this.__highWaterMark && !this.__readyResolve) {
                this.__ready = new Promise((resolve, reject) => {
                    this.__readyResolve = resolve;
                    this.__readyReject = reject;
                });
                this.__ready.catch(() => {});
            } else if (this.__queueSize < this.__highWaterMark && this.__readyResolve) {
                this.__readyResolve();
                this.__readyResolve = this.__readyReject = null;
            }
        }
        __write(chunk, writer = this.__writer) {
            if (this.__state === 'erroring' || this.__state === 'errored')
                return Promise.reject(this.__storedError);
            if (this.__state !== 'writable') return reject('WritableStream is not writable');
            let size;
            try {
                const sizeAlgorithm = this.__size;
                size = Number(sizeAlgorithm(chunk));
                if (!Number.isFinite(size) || size < 0) throw new RangeError('Invalid chunk size');
            } catch (error) {
                const failure = Promise.reject(error);
                this.__error(error);
                return failure;
            }
            // size() may release this writer, close the stream, or error it.
            if (writer && writer.__stream !== this) return reject('Writer was released');
            if (this.__state === 'erroring' || this.__state === 'errored')
                return Promise.reject(this.__storedError);
            if (this.__state !== 'writable') return reject('WritableStream is not writable');
            this.__queueSize += size;
            this.__updateBackpressure();
            const operation = new Promise((resolve, reject) => {
                this.__queue.push({ kind: 'write', chunk, size, resolve, reject });
            });
            this.__drain();
            return operation;
        }
        close() {
            if (this.locked) return reject('Cannot close a locked WritableStream');
            return this.__close();
        }
        __close() {
            if (this.__state === 'erroring')
                return new Promise((_, reject) => this.__erroringCloseRequests.push(reject));
            if (this.__state === 'errored' && this.__aborted)
                return Promise.reject(this.__storedError);
            if (this.__state === 'errored') return reject('WritableStream is not writable');
            if (this.__state !== 'writable') return reject('WritableStream is not writable');
            this.__state = 'closing';
            if (this.__readyResolve) {
                this.__readyResolve();
                this.__readyResolve = this.__readyReject = null;
            }
            const operation = new Promise((resolve, reject) => {
                this.__queue.push({ kind: 'close', resolve, reject });
            });
            this.__drain();
            return operation;
        }
        __drain() {
            if (!this.__started || this.__processing) return;
            if (this.__state === 'erroring') { this.__finishErroring(); return; }
            if (this.__state === 'errored' || !this.__queue.length) return;
            const item = this.__queue.shift();
            this.__processing = item;
            let result;
            try {
                result = item.kind === 'write'
                    ? (this.__sinkWrite === undefined ? undefined
                        : Reflect.apply(this.__sinkWrite, this.__sink, [item.chunk, this.__controller]))
                    : (this.__sinkClose === undefined ? undefined
                        : Reflect.apply(this.__sinkClose, this.__sink, []));
            } catch (error) {
                this.__processing = null;
                item.reject(error);
                this.__error(error);
                this.__drain();
                return;
            }
            Promise.resolve(result).then(() => {
                this.__processing = null;
                if (item.kind === 'close') {
                    if (this.__state !== 'errored') this.__state = 'closed';
                } else {
                    this.__queueSize -= item.size;
                }
                item.resolve();
                if (this.__closeAbort && item.kind === 'close') {
                    this.__closeAbort.resolve();
                    this.__closeAbort = null;
                }
                if (item.kind === 'close' && this.__state === 'closed') this.__closeResolve();
                if (item.kind === 'write') this.__updateBackpressure();
                this.__drain();
            }, error => {
                this.__processing = null;
                item.reject(error);
                if (this.__closeAbort && item.kind === 'close') {
                    this.__closeAbort.reject(error);
                    const abortReason = this.__closeAbort.reason;
                    this.__closeAbort = null;
                    this.__error(abortReason);
                } else this.__error(error);
                this.__drain();
            });
        }
        abort(reason) {
            if (this.locked) return reject('Cannot abort a locked WritableStream');
            return this.__abort(reason);
        }
        __abort(reason) {
            if (this.__state === 'closed') return Promise.resolve();
            if (this.__state === 'errored') return Promise.resolve();
            // Signaling abort runs author callbacks synchronously. They can close
            // the stream or start another abort, so re-check its state below.
            this.__abortController.abort(reason);
            if (this.__state === 'closed' || this.__state === 'errored')
                return Promise.resolve();
            if (this.__pendingAbort) {
                return this.__pendingAbort.promise;
            }
            if (this.__closeAbort) return this.__closeAbort.promise;
            if (this.__state === 'closing' && this.__processing?.kind === 'close') {
                let resolve, reject;
                const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
                this.__closeAbort = { promise, resolve, reject, reason };
                if (this.__closeErrorReason === undefined) this.__closeErrorReason = reason;
                this.__rejectReady(this.__closeErrorReason);
                return promise;
            }
            let resolve, reject;
            const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
            this.__pendingAbort = { promise, resolve, reject, reason,
                wasErroring: this.__state === 'erroring' };
            this.__aborted = true;
            this.__error(reason);
            return promise;
        }
        __rejectReady(reason) {
            if (this.__readyReject) this.__readyReject(reason);
            else {
                this.__ready = Promise.reject(reason);
                this.__ready.catch(() => {});
            }
            this.__readyResolve = this.__readyReject = null;
        }
        __error(reason) {
            if (this.__state === 'closed' || this.__state === 'errored' ||
                this.__state === 'erroring') return;
            if (this.__state === 'closing' && this.__processing?.kind === 'close') {
                if (this.__closeErrorReason === undefined) this.__closeErrorReason = reason;
                this.__rejectReady(this.__closeErrorReason);
                return;
            }
            this.__state = 'erroring';
            this.__storedError = reason;
            this.__rejectReady(reason);
            this.__drain();
        }
        __finishErroring() {
            if (this.__state !== 'erroring' || this.__processing) return;
            this.__state = 'errored';
            const reason = this.__storedError;
            for (const item of this.__queue.splice(0)) item.reject(reason);
            this.__queueSize = 0;
            const rejectLateCloses = () => {
                for (const reject of this.__erroringCloseRequests.splice(0)) reject(reason);
            };
            const pending = this.__pendingAbort;
            this.__pendingAbort = null;
            if (pending) {
                if (pending.wasErroring) {
                    pending.reject(reason);
                    rejectLateCloses();
                    this.__closeReject(reason);
                } else {
                    let aborted;
                    try { aborted = this.__sinkAbort === undefined ? undefined
                        : Reflect.apply(this.__sinkAbort, this.__sink, [pending.reason]); }
                    catch (error) { aborted = Promise.reject(error); }
                    this.__abortSettling = true;
                    Promise.resolve(aborted).then(() => {
                        this.__abortSettling = false;
                        pending.resolve(); rejectLateCloses(); this.__closeReject(reason);
                    }, error => {
                        this.__abortSettling = false;
                        pending.reject(error); rejectLateCloses(); this.__closeReject(reason);
                    });
                }
            } else { rejectLateCloses(); this.__closeReject(reason); }
        }
    }

    Object.assign(globalThis, {
        WritableStream, WritableStreamDefaultWriter, WritableStreamDefaultController
    });
})();
