(() => {
    'use strict';

    class TransformStreamDefaultController {
        constructor(stream) { this.__stream = stream; }
        get desiredSize() { return this.__stream.readable.__controller.desiredSize; }
        enqueue(chunk) {
            const stream = this.__stream;
            const wasReadable = stream.readable.__state === 'readable';
            try { stream.readable.__controller.enqueue(chunk); }
            catch (error) {
                // A size callback may call controller.error() and then throw.
                // The first stored stream error wins over the later exception.
                if (wasReadable && stream.readable.__state === 'errored') {
                    stream.writable?.__error(stream.readable.__storedError);
                    stream.__unblock();
                    throw stream.readable.__storedError;
                }
                throw error;
            }
            if (this.desiredSize <= 0) stream.__setBackpressure();
        }
        error(reason) {
            const stream = this.__stream;
            if (stream.__cancelCallbackActive && !stream.__cancelCallbackErrored) {
                stream.__cancelCallbackErrored = true;
                stream.__cancelCallbackError = reason;
            }
            stream.readable.__controller.error(reason);
            stream.writable?.__error(reason);
            stream.__unblock();
        }
        terminate() {
            const stream = this.__stream;
            if (stream.readable.__state === 'readable' &&
                !stream.readable.__closeRequested)
                stream.readable.__controller.close();
            if (stream.writable?.__state === 'writable' ||
                stream.writable?.__state === 'closing')
                stream.writable.__error(new TypeError('TransformStream was terminated'));
            stream.__unblock();
        }
    }

    class TransformStream {
        constructor(transformer = {}, writableStrategy = {}, readableStrategy = {}) {
            transformer = Object(transformer);
            if (transformer.readableType !== undefined || transformer.writableType !== undefined)
                throw new RangeError('TransformStream types are not supported');
            this.__backpressure = false;
            this.__backpressurePromise = Promise.resolve();
            this.__resume = null;
            this.__cancelled = false;
            this.__cancelCallbackActive = false;
            this.__cancelCallbackErrored = false;
            this.__finishPromise = null;
            this.__controller = new TransformStreamDefaultController(this);
            const finish = (mode, reason) => {
                if (this.__finishPromise) return this.__finishPromise;
                let resolveFinish, rejectFinish;
                this.__finishPromise = new Promise((resolve, reject) => {
                    resolveFinish = resolve; rejectFinish = reject;
                });
                if (mode !== 'close') {
                    this.__cancelled = true;
                    this.__unblock();
                }
                let result;
                const abortedBeforeCancel = this.writable?.__abortController.signal.aborted;
                this.__cancelCallbackActive = mode === 'source';
                try { result = mode === 'close'
                    ? transformer.flush?.(this.__controller)
                    : transformer.cancel?.(reason); }
                catch (error) { result = Promise.reject(error); }
                finally {
                    this.__cancelCallbackActive = false;
                    this.__abortInsideCancel = mode === 'source' && !abortedBeforeCancel &&
                        this.writable.__abortController.signal.aborted;
                }
                Promise.resolve(result).then(() => {
                    if (mode === 'source') {
                        // An error raised by cancel() itself wins; an unrelated
                        // abort or controller error that races after cancellation
                        // started must not replace its completion result.
                        if (this.__cancelCallbackErrored) {
                            rejectFinish(this.__cancelCallbackError);
                        } else if (this.__abortInsideCancel) {
                            rejectFinish(this.writable.__abortController.signal.reason);
                        } else {
                            this.writable.__error(reason);
                            resolveFinish();
                        }
                    } else if (this.readable.__state === 'errored') {
                        rejectFinish(this.readable.__storedError);
                    } else {
                        if (mode === 'abort') this.readable.__controller.error(reason);
                        else if (this.readable.__state === 'readable')
                            this.readable.__controller.close();
                        resolveFinish();
                    }
                }, error => {
                    if (mode === 'source') this.writable.__error(error);
                    else this.readable.__controller.error(error);
                    this.__unblock();
                    rejectFinish(error);
                });
                return this.__finishPromise;
            };
            let resolveStart, rejectStart;
            const startPromise = new Promise((resolve, reject) => {
                resolveStart = resolve; rejectStart = reject;
            });
            this.__readable = new ReadableStream({
                start: () => startPromise,
                pull: () => { this.__unblock(); },
                cancel: reason => finish('source', reason)
            }, { highWaterMark: 0, ...Object(readableStrategy) });
            if (this.readable.__controller.desiredSize <= 0) this.__setBackpressure();
            this.__writable = new WritableStream({
                start: () => startPromise,
                write: async chunk => {
                    if (this.__backpressure) await this.__backpressurePromise;
                    if (this.writable.__state === 'erroring' ||
                        this.writable.__state === 'errored')
                        throw this.writable.__storedError;
                    if (this.__cancelled) return;
                    try {
                        if (transformer.transform)
                            await transformer.transform(chunk, this.__controller);
                        else this.__controller.enqueue(chunk);
                    } catch (error) {
                        this.__controller.error(error);
                        throw error;
                    }
                },
                close: () => finish('close'),
                abort: reason => finish('abort', reason)
            }, { highWaterMark: 1, ...Object(writableStrategy) });
            try { resolveStart(transformer.start?.(this.__controller)); }
            catch (error) { rejectStart(error); throw error; }
        }
        get readable() { return this.__readable; }
        get writable() { return this.__writable; }
        __setBackpressure() {
            if (this.__backpressure) return;
            this.__backpressure = true;
            this.__backpressurePromise = new Promise(resolve => { this.__resume = resolve; });
        }
        __unblock() {
            if (!this.__backpressure) return;
            this.__backpressure = false;
            this.__resume?.();
            this.__resume = null;
        }
    }

    Object.assign(globalThis, { TransformStream, TransformStreamDefaultController });
})();
