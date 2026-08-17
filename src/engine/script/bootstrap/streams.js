(() => {
    'use strict';
    const typeError = message => Promise.reject(new TypeError(message));

    class ReadableStreamDefaultController {
        constructor(stream) { this.__stream = stream; }
        get desiredSize() {
            const stream = this.__stream;
            return stream.__state !== 'readable' || stream.__closeRequested
                ? null : stream.__highWaterMark - stream.__queue.length;
        }
        enqueue(chunk) {
            const stream = this.__stream;
            if (stream.__state !== 'readable' || stream.__closeRequested)
                throw new TypeError('ReadableStream is not in a state that permits enqueue');
            const pending = stream.__reads.shift();
            if (pending) pending.resolve({ value: chunk, done: false });
            else stream.__queue.push(chunk);
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
        constructor(stream) { this.__stream = stream; }
        get closed() {
            return this.__stream ? this.__stream.__closedPromise
                : Promise.reject(new TypeError('Reader was released'));
        }
        read() {
            const stream = this.__stream;
            if (!stream) return typeError('Reader was released');
            stream.__disturb();
            if (stream.__queue.length) {
                const value = stream.__queue.shift();
                if (stream.__closeRequested && !stream.__queue.length) stream.__finishClose();
                else stream.__pullIfNeeded();
                return Promise.resolve({ value, done: false });
            }
            if (stream.__state === 'closed')
                return Promise.resolve({ value: undefined, done: true });
            if (stream.__state === 'errored') return Promise.reject(stream.__storedError);
            const request = {};
            const promise = new Promise((resolve, reject) => {
                request.resolve = resolve; request.reject = reject;
            });
            stream.__reads.push(request);
            stream.__pullIfNeeded();
            return promise;
        }
        cancel(reason) {
            return this.__stream ? this.__stream.__cancel(reason) : typeError('Reader was released');
        }
        releaseLock() {
            const stream = this.__stream;
            if (!stream) return;
            if (stream.__reads.length) throw new TypeError('Cannot release a reader with pending reads');
            stream.__reader = null; this.__stream = null;
        }
    }

    class ReadableStream {
        constructor(source = {}, strategy = {}) {
            source = Object(source); strategy = Object(strategy);
            if (source.type !== undefined) throw new RangeError('Only default ReadableStream sources are supported');
            this.__state = 'readable'; this.__storedError = undefined;
            this.__queue = []; this.__reads = []; this.__reader = null;
            this.__disturbed = false; this.__closeRequested = false;
            this.__started = false; this.__pulling = false; this.__pullAgain = false;
            this.__source = source;
            this.__highWaterMark = strategy.highWaterMark === undefined ? 1 : Number(strategy.highWaterMark);
            if (!Number.isFinite(this.__highWaterMark) || this.__highWaterMark < 0)
                throw new RangeError('highWaterMark must be a finite non-negative number');
            this.__closedPromise = new Promise((resolve, reject) => {
                this.__closeResolve = resolve; this.__closeReject = reject;
            });
            this.__controller = new ReadableStreamDefaultController(this);
            let started;
            try { started = source.start?.(this.__controller); }
            catch (error) { this.__finishError(error); return; }
            Promise.resolve(started).then(() => {
                this.__started = true; this.__pullIfNeeded();
            }, error => this.__finishError(error));
        }
        get locked() { return this.__reader !== null; }
        getReader(options = undefined) {
            if (options?.mode !== undefined) throw new RangeError('BYOB readers are not supported');
            if (this.locked) throw new TypeError('ReadableStream is locked');
            const reader = new ReadableStreamDefaultReader(this); this.__reader = reader; return reader;
        }
        cancel(reason) {
            if (this.locked) return typeError('Cannot cancel a locked ReadableStream');
            return this.__cancel(reason);
        }
        __disturb() {
            if (!this.__disturbed) { this.__disturbed = true; this.__onDisturb?.(); }
        }
        __cancel(reason) {
            this.__disturb(); this.__queue.length = 0;
            if (this.__state === 'closed') return Promise.resolve();
            if (this.__state === 'errored') return Promise.reject(this.__storedError);
            this.__finishClose();
            try { return Promise.resolve(this.__source.cancel?.(reason)).then(() => undefined); }
            catch (error) { return Promise.reject(error); }
        }
        __finishClose() {
            if (this.__state !== 'readable') return;
            this.__state = 'closed';
            for (const pending of this.__reads.splice(0))
                pending.resolve({ value: undefined, done: true });
            this.__closeResolve();
        }
        __finishError(reason) {
            if (this.__state !== 'readable') return;
            this.__state = 'errored'; this.__storedError = reason; this.__queue.length = 0;
            for (const pending of this.__reads.splice(0)) pending.reject(reason);
            this.__closeReject(reason);
        }
        __pullIfNeeded() {
            if (!this.__started || this.__state !== 'readable' || this.__closeRequested ||
                typeof this.__source.pull !== 'function' ||
                (!this.__reads.length && this.__controller.desiredSize <= 0)) return;
            if (this.__pulling) { this.__pullAgain = true; return; }
            this.__pulling = true;
            let result;
            try { result = this.__source.pull(this.__controller); }
            catch (error) { this.__pulling = false; this.__finishError(error); return; }
            Promise.resolve(result).then(() => {
                this.__pulling = false;
                if (this.__pullAgain) { this.__pullAgain = false; this.__pullIfNeeded(); }
            }, error => { this.__pulling = false; this.__finishError(error); });
        }
        __tee(cloneSecondBranch) {
            if (this.locked) throw new TypeError('ReadableStream is locked');
            const reader = this.getReader(); const branches = [{}, {}];
            const streams = branches.map(branch => new ReadableStream({
                start(controller) { branch.controller = controller; },
                cancel() { branch.cancelled = true; if (branches.every(item => item.cancelled)) return reader.cancel(); }
            }));
            const pump = () => reader.read().then(({ value, done }) => {
                if (done) { for (const branch of branches) if (!branch.cancelled) branch.controller.close(); return; }
                let secondValue = value;
                if (cloneSecondBranch && !branches[1].cancelled) {
                    try { secondValue = structuredClone(value); }
                    catch (error) {
                        for (const branch of branches) if (!branch.cancelled) branch.controller.error(error);
                        return reader.cancel(error);
                    }
                }
                if (!branches[0].cancelled) branches[0].controller.enqueue(value);
                if (!branches[1].cancelled) branches[1].controller.enqueue(secondValue);
                return pump();
            }, error => { for (const branch of branches) if (!branch.cancelled) branch.controller.error(error); });
            pump(); return streams;
        }
        tee() { return this.__tee(false); }
        pipeTo(destination) {
            if (!(destination instanceof WritableStream)) return typeError('pipeTo requires a WritableStream');
            if (this.locked || destination.locked) return typeError('Cannot pipe a locked stream');
            const reader = this.getReader(), writer = destination.getWriter();
            const pump = () => reader.read().then(({ value, done }) =>
                done ? writer.close() : writer.write(value).then(pump));
            return pump().catch(error => writer.abort(error).then(() => { throw error; }))
                .finally(() => { reader.releaseLock(); writer.releaseLock(); });
        }
        pipeThrough(transform) {
            if (!transform?.readable || !transform?.writable) throw new TypeError('Invalid transform pair');
            this.pipeTo(transform.writable).catch(() => {}); return transform.readable;
        }
        values(options = {}) {
            const reader = this.getReader(); const preventCancel = !!options.preventCancel;
            return {
                next: () => reader.read(),
                return: value => (preventCancel ? Promise.resolve() : reader.cancel()).then(() => {
                    reader.releaseLock(); return { value, done: true };
                }),
                [Symbol.asyncIterator]() { return this; }
            };
        }
        [Symbol.asyncIterator]() { return this.values(); }
        static from(iterable) {
            const iterator = iterable?.[Symbol.asyncIterator]?.() || iterable?.[Symbol.iterator]?.();
            if (!iterator) throw new TypeError('ReadableStream.from requires an iterable');
            return new ReadableStream({
                pull(controller) {
                    return Promise.resolve(iterator.next()).then(result => {
                        if (result.done) controller.close(); else controller.enqueue(result.value);
                    });
                },
                cancel(reason) { return iterator.return?.(reason); }
            });
        }
    }

    class WritableStreamDefaultWriter {
        constructor(stream) { this.__stream = stream; }
        get closed() { return this.__stream ? this.__stream.__closedPromise : typeError('Writer was released'); }
        get ready() { return this.__stream ? this.__stream.__ready : typeError('Writer was released'); }
        get desiredSize() { return this.__stream?.__state === 'writable' ? 1 : null; }
        write(chunk) { return this.__stream ? this.__stream.__write(chunk) : typeError('Writer was released'); }
        close() { return this.__stream ? this.__stream.close() : typeError('Writer was released'); }
        abort(reason) { return this.__stream ? this.__stream.abort(reason) : typeError('Writer was released'); }
        releaseLock() { if (this.__stream) { this.__stream.__writer = null; this.__stream = null; } }
    }
    class WritableStream {
        constructor(sink = {}, _strategy = {}) {
            this.__sink = Object(sink); this.__writer = null; this.__state = 'writable';
            this.__ready = Promise.resolve(); this.__tail = Promise.resolve();
            this.__closedPromise = new Promise((resolve, reject) => {
                this.__closeResolve = resolve; this.__closeReject = reject;
            });
            try { this.__tail = Promise.resolve(this.__sink.start?.(this)); }
            catch (error) { this.__error(error); }
        }
        get locked() { return this.__writer !== null; }
        getWriter() {
            if (this.locked) throw new TypeError('WritableStream is locked');
            const writer = new WritableStreamDefaultWriter(this); this.__writer = writer; return writer;
        }
        __write(chunk) {
            if (this.__state !== 'writable') return typeError('WritableStream is not writable');
            this.__tail = this.__tail.then(() => this.__sink.write?.(chunk, this));
            this.__tail.catch(error => this.__error(error)); return this.__tail;
        }
        close() {
            if (this.__state !== 'writable') return typeError('WritableStream is not writable');
            this.__state = 'closing';
            this.__tail = this.__tail.then(() => this.__sink.close?.()).then(() => {
                this.__state = 'closed'; this.__closeResolve();
            }, error => { this.__error(error); throw error; });
            return this.__tail;
        }
        abort(reason) {
            if (this.__state === 'closed') return Promise.resolve();
            if (this.__state === 'errored') return Promise.reject(this.__storedError);
            this.__state = 'errored'; this.__storedError = reason;
            this.__closeReject(reason);
            try { return Promise.resolve(this.__sink.abort?.(reason)).then(() => undefined); }
            catch (error) { return Promise.reject(error); }
        }
        __error(reason) {
            if (this.__state === 'closed' || this.__state === 'errored') return;
            this.__state = 'errored'; this.__storedError = reason; this.__closeReject(reason);
        }
    }
    class TransformStream {
        constructor(transformer = {}) {
            this.readable = new ReadableStream({ start: controller => { this.__controller = controller; } });
            this.writable = new WritableStream({
                write: chunk => {
                    return transformer.transform
                        ? transformer.transform(chunk, this.__controller)
                        : this.__controller.enqueue(chunk);
                },
                close: () => Promise.resolve(transformer.flush?.(this.__controller))
                    .then(() => this.__controller.close()),
                abort: reason => { this.__controller.error(reason); }
            });
        }
    }
    Object.assign(globalThis, {
        ReadableStream, ReadableStreamDefaultReader, ReadableStreamDefaultController,
        WritableStream, WritableStreamDefaultWriter, TransformStream
    });
})();
