(() => {
    'use strict';
    const readInternal = ReadableStreamDefaultReader.prototype.__readInternal;
    const releaseReader = ReadableStreamDefaultReader.prototype.releaseLock;
    const releaseWriter = WritableStreamDefaultWriter.prototype.releaseLock;
    const getReader = ReadableStream.prototype.getReader;
    const getWriter = WritableStream.prototype.getWriter;
    const pipeOptions = options => {
        options = Object(options);
        const preventAbort = !!options.preventAbort;
        const preventCancel = !!options.preventCancel;
        const preventClose = !!options.preventClose;
        const signal = options.signal;
        if (signal !== undefined && !AbortSignal.__isSignal(signal))
            throw new TypeError('signal must be an AbortSignal');
        return { preventAbort, preventCancel, preventClose, signal };
    };

    // Streams Standard §4.9.1: shutdown is a one-way transition. Pending writes
    // must settle before propagating a close, cancel, abort, or original error.
    const pipeTo = function pipeTo(destination, options = {}) {
        if (!(this instanceof ReadableStream) || !Object.hasOwn(this, '__state'))
            return Promise.reject(new TypeError('pipeTo requires a ReadableStream'));
        if (!(destination instanceof WritableStream) || !Object.hasOwn(destination, '__state'))
            return Promise.reject(new TypeError('pipeTo requires a WritableStream'));
        let preventClose, preventAbort, preventCancel, signal;
        try {
            ({ preventAbort, preventCancel, preventClose, signal } = pipeOptions(options));
        } catch (error) { return Promise.reject(error); }
        if (this.locked || destination.locked)
            return Promise.reject(new TypeError('Cannot pipe a locked stream'));

        const source = this;
        const reader = getReader.call(source);
        const writer = getWriter.call(destination);
        source.__disturb();
        let shuttingDown = false, finalized = false, reading = false;
        let sourceClosePending = false;
        let abortListener;
        const writes = new Set();
        let resolvePipe, rejectPipe;
        const result = new Promise((resolve, reject) => {
            resolvePipe = resolve; rejectPipe = reject;
        });
        const finish = (error, hasError) => {
            if (finalized) return;
            finalized = true;
            if (signal && abortListener) signal.removeEventListener('abort', abortListener);
            releaseReader.call(reader);
            releaseWriter.call(writer);
            if (hasError) rejectPipe(error);
            else resolvePipe();
        };
        const shutdown = (action, error, hasError = false, observeWriteErrors = false) => {
            if (shuttingDown) return;
            shuttingDown = true;
            let failedWrite, writeFailed = false;
            // Resolve only with undefined: adopting an array of settled-write
            // records would consult an author-defined Object.prototype.then.
            const waitForWrites = () => new Promise(resolve => {
                const pending = [...writes];
                if (!pending.length) { resolve(); return; }
                let remaining = pending.length;
                const complete = () => { if (--remaining === 0) resolve(); };
                for (const write of pending) write.then(complete, reason => {
                    if (!writeFailed) { failedWrite = reason; writeFailed = true; }
                    complete();
                });
            });
            const continueShutdown = () => {
                if (observeWriteErrors && writeFailed) {
                    finish(failedWrite, true); return;
                }
                if (!action) { finish(error, hasError); return; }
                let operation;
                try { operation = action(); }
                catch (failure) { finish(failure, true); return; }
                Promise.resolve(operation).then(() => finish(error, hasError),
                    failure => finish(failure, true));
            };
            if (writes.size || destination.__state === 'writable')
                waitForWrites().then(continueShutdown);
            else continueShutdown();
        };
        const onAbort = (initial = false) => {
            // A signal supplied already aborted takes priority at pipe start;
            // a later abort cannot replace shutdown after the source closes.
            if (!initial && source.__state === 'closed') return;
            const reason = signal.reason;
            shutdown(() => {
                const actions = [];
                if (!preventAbort && destination.__state === 'writable')
                    actions.push(destination.__abort(reason));
                if (!preventCancel && source.__state === 'readable')
                    actions.push(source.__cancel(reason));
                return Promise.all(actions);
            }, reason, true);
        };
        if (signal?.aborted) { onAbort(true); return result; }
        abortListener = () => onAbort(false);
        signal?.addEventListener('abort', abortListener);

        // Source failures precede destination failures when both are already
        // terminal. Observe both closed promises to wake a backpressured pipe.
        const sourceError = error => {
            shutdown(preventAbort ? null : () => destination.__abort(error), error, true);
        };
        const destinationError = error => {
            shutdown(preventCancel ? null : () => source.__cancel(error), error, true);
        };
        const sourceClosed = () => {
            // A reader can become closed while its final chunk's read promise
            // is still queued for delivery. Write that chunk before shutdown.
            if (reading) { sourceClosePending = true; return; }
            shutdown(preventClose ? null : () => {
                if (destination.__state === 'closing') return destination.__closedPromise;
                if (destination.__state === 'closed') return;
                return destination.__close();
            },
                undefined, false, true);
        };
        const destinationClosed = () => {
            const error = new TypeError('Destination closed before source completed');
            shutdown(preventCancel ? null : () => source.__cancel(error), error, true);
        };

        if (source.__state === 'errored') sourceError(source.__storedError);
        else if (destination.__state === 'errored') destinationError(destination.__storedError);
        else if (source.__state === 'closed') sourceClosed();
        else if (destination.__state === 'closed' || destination.__state === 'closing')
            destinationClosed();

        reader.closed.then(sourceClosed, sourceError);
        writer.closed.then(destinationClosed, destinationError);
        (async () => {
            while (!shuttingDown) {
                try { await writer.ready; }
                catch (error) { if (!shuttingDown) destinationError(error); break; }
                if (shuttingDown) break;
                // The ready promise may have settled before a preceding write
                // changed desiredSize. Never read against destination pressure.
                if (writer.desiredSize <= 0) continue;
                let value, done = false;
                reading = true;
                try {
                    await new Promise((resolve, reject) => readInternal.call(reader,
                        chunk => { value = chunk; resolve(); },
                        () => { done = true; resolve(); }, reject));
                }
                catch (error) {
                    reading = false;
                    if (!shuttingDown) sourceError(error);
                    break;
                }
                if (shuttingDown) { reading = false; break; }
                if (done) { reading = false; sourceClosed(); break; }
                let writing;
                try { writing = destination.__write(value); }
                catch (error) { reading = false; destinationError(error); break; }
                writes.add(writing);
                writing.then(() => writes.delete(writing), error => {
                    writes.delete(writing);
                    if (!shuttingDown) destinationError(error);
                });
                reading = false;
                if (sourceClosePending) { sourceClosed(); break; }
            }
        })().catch(error => { if (!shuttingDown) shutdown(null, error, true); });
        return result;
    };
    ReadableStream.prototype.pipeTo = pipeTo;

    ReadableStream.prototype.pipeThrough = function pipeThrough(transform, options = {}) {
        if (!(this instanceof ReadableStream) || !Object.hasOwn(this, '__state'))
            throw new TypeError('pipeThrough requires a ReadableStream');
        if (transform === null || transform === undefined)
            throw new TypeError('pipeThrough requires a transform pair');
        // Web IDL converts ReadableWritablePair before the options dictionary.
        const readable = transform.readable;
        if (!(readable instanceof ReadableStream) || !Object.hasOwn(readable, '__state'))
            throw new TypeError('Transform readable must be a ReadableStream');
        const writable = transform.writable;
        if (!(writable instanceof WritableStream) || !Object.hasOwn(writable, '__state'))
            throw new TypeError('Transform writable must be a WritableStream');
        const converted = pipeOptions(options);
        if (this.locked || writable.locked) throw new TypeError('Cannot pipe a locked stream');
        // Call the internal algorithm, not an author-overridable pipeTo method.
        pipeTo.call(this, writable, converted).catch(error => {
            readable.__finishError(error);
        });
        return readable;
    };
})();
