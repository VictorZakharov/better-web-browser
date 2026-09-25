(() => {
    'use strict';

    const reject = message => Promise.reject(new TypeError(message));
    const transferred = buffer => structuredClone(buffer, { transfer: [buffer] });
    const assertAttached = buffer => {
        // A detached ArrayBuffer reports byteLength 0 just like an empty one.
        // Constructing a zero-length view distinguishes the two without a copy.
        try { new Uint8Array(buffer, 0, 0); }
        catch { throw new TypeError('BYOB request buffer is detached'); }
    };
    const validView = view => ArrayBuffer.isView(view) && view.byteLength > 0 &&
        view.buffer instanceof ArrayBuffer && view.buffer.byteLength > 0;

    const resultView = (descriptor, length) => descriptor.type === DataView
        ? new DataView(descriptor.buffer, descriptor.offset, length)
        : new descriptor.type(descriptor.buffer, descriptor.offset,
            length / descriptor.elementSize);

    const settle = (stream, descriptor, done) => {
        const index = stream.__byobReads.indexOf(descriptor);
        if (index >= 0) stream.__byobReads.splice(index, 1);
        descriptor.request?.__invalidate();
        if (descriptor.orphan) {
            if (descriptor.filled) {
                const value = resultView(descriptor, descriptor.filled);
                if (stream.__reads.length) {
                    stream.__reads.shift().resolve({ value, done: false });
                } else {
                    const copy = new Uint8Array(new Uint8Array(descriptor.buffer,
                        descriptor.offset, descriptor.filled));
                    stream.__queue.unshift({ value: copy, size: copy.byteLength });
                    stream.__queueSize += copy.byteLength;
                }
            }
        } else if (descriptor.auto) {
            const value = resultView(descriptor, descriptor.filled);
            const pending = stream.__reads.shift();
            pending?.resolve({ value: done ? undefined : value, done });
        } else {
            descriptor.resolve({ value: resultView(descriptor, descriptor.filled), done });
        }
        if (stream.__queue.length && stream.__byobReads.length)
            fill(stream, stream.__byobReads[0]);
        if (stream.__closeRequested && !stream.__queue.length &&
            (done || !stream.__byobReads.length))
            stream.__finishClose();
        else stream.__pullIfNeeded();
    };

    const commitAligned = (stream, descriptor) => {
        const completeBytes = descriptor.filled - descriptor.filled % descriptor.elementSize;
        if (completeBytes < descriptor.minBytes) return false;
        if (completeBytes < descriptor.filled) {
            const remainder = new Uint8Array(descriptor.buffer,
                descriptor.offset + completeBytes, descriptor.filled - completeBytes);
            const copy = new Uint8Array(remainder);
            stream.__queue.unshift({ value: copy, size: copy.byteLength });
            stream.__queueSize += copy.byteLength;
            descriptor.filled = completeBytes;
        }
        settle(stream, descriptor, false);
        return true;
    };

    const fill = (stream, descriptor) => {
        while (stream.__queue.length && descriptor.filled < descriptor.capacity) {
            const entry = stream.__queue[0];
            const amount = Math.min(entry.value.byteLength,
                descriptor.capacity - descriptor.filled);
            new Uint8Array(descriptor.buffer,
                descriptor.offset + descriptor.filled, amount)
                .set(entry.value.subarray(0, amount));
            descriptor.filled += amount;
            stream.__queueSize -= amount;
            if (amount === entry.value.byteLength) stream.__queue.shift();
            else entry.value = entry.value.subarray(amount);
        }
        if (!commitAligned(stream, descriptor) &&
            stream.__closeRequested && !stream.__queue.length) {
            // A partially filled active BYOB request can still be acknowledged
            // with respond(0) after close().
            if (descriptor.filled % descriptor.elementSize) {
                stream.__finishError(new TypeError('Incomplete BYOB element at end of stream'));
            }
        }
    };

    const makeDescriptor = (stream, view, min, auto = false) => {
        const type = view.constructor;
        const elementSize = type === DataView ? 1 : type.BYTES_PER_ELEMENT;
        const capacity = view.byteLength;
        const bufferLength = view.buffer.byteLength;
        const descriptor = {
            type, elementSize, capacity, offset: view.byteOffset,
            buffer: transferred(view.buffer), bufferLength,
            filled: 0,
            minBytes: min * elementSize, auto, request: null
        };
        descriptor.promise = new Promise((resolve, reject) => {
            descriptor.resolve = resolve; descriptor.reject = reject;
        });
        stream.__byobReads.push(descriptor);
        fill(stream, descriptor);
        if (stream.__state === 'readable' && stream.__byobReads.includes(descriptor))
            stream.__pullIfNeeded();
        return descriptor;
    };

    class ReadableStreamBYOBRequest {
        constructor(controller, descriptor) {
            this.__controller = controller;
            this.__descriptor = descriptor;
        }
        get view() {
            const descriptor = this.__descriptor;
            if (!descriptor) return null;
            return new Uint8Array(descriptor.buffer,
                descriptor.offset + descriptor.filled,
                descriptor.capacity - descriptor.filled);
        }
        respond(bytesWritten) {
            const descriptor = this.__descriptor;
            if (!descriptor) throw new TypeError('BYOB request is no longer active');
            assertAttached(descriptor.buffer);
            const count = Number(bytesWritten);
            if (!Number.isInteger(count) || count < 0 ||
                count > descriptor.capacity - descriptor.filled ||
                (count === 0 && this.__controller.__stream.__state === 'readable' &&
                    !this.__controller.__stream.__closeRequested))
                throw new RangeError('Invalid number of bytes written');
            descriptor.buffer = transferred(descriptor.buffer);
            descriptor.filled += count;
            this.__invalidate();
            const stream = this.__controller.__stream;
            if (!commitAligned(stream, descriptor) &&
                (stream.__closeRequested || stream.__state === 'closed')) {
                if (descriptor.filled % descriptor.elementSize)
                    stream.__finishError(new TypeError('Incomplete BYOB element at end of stream'));
                else settle(stream, descriptor, true);
            } else {
                stream.__pullIfNeeded();
            }
        }
        respondWithNewView(view) {
            const descriptor = this.__descriptor;
            if (!descriptor) throw new TypeError('BYOB request is no longer active');
            if (!ArrayBuffer.isView(view))
                throw new TypeError('New BYOB response must be an ArrayBuffer view');
            assertAttached(view.buffer);
            const stream = this.__controller.__stream;
            if (stream.__state === 'readable' && !stream.__closeRequested &&
                view.byteLength === 0)
                throw new TypeError('Readable byte streams require a nonempty BYOB response');
            if ((view.buffer !== descriptor.buffer &&
                view.buffer.byteLength !== descriptor.bufferLength) ||
                view.byteOffset !== descriptor.offset + descriptor.filled)
                throw new RangeError('New view must start at the active BYOB request offset');
            if (stream.__closeRequested || stream.__state === 'closed') {
                if (view.byteLength !== 0)
                    throw new TypeError('Closed byte streams require an empty BYOB response');
            }
            descriptor.buffer = view.buffer;
            this.respond(view.byteLength);
        }
        __invalidate() {
            if (this.__descriptor) this.__descriptor.request = null;
            this.__descriptor = null;
        }
    }

    class ReadableByteStreamController {
        constructor(stream, source) {
            this.__stream = stream;
            const size = source.autoAllocateChunkSize;
            if (size !== undefined && (!Number.isSafeInteger(Number(size)) || Number(size) <= 0))
                throw new TypeError('autoAllocateChunkSize must be a positive integer');
            this.__autoAllocateChunkSize = size === undefined ? undefined : Number(size);
        }
        get desiredSize() {
            const stream = this.__stream;
            if (stream.__state === 'errored') return null;
            return stream.__state === 'closed' ? 0 : stream.__highWaterMark - stream.__queueSize;
        }
        get byobRequest() {
            const stream = this.__stream;
            if (stream.__state !== 'readable') return null;
            let descriptor = stream.__byobReads[0];
            if (!descriptor && this.__autoAllocateChunkSize && stream.__reads.length) {
                descriptor = makeDescriptor(stream,
                    new Uint8Array(this.__autoAllocateChunkSize), 1, true);
            }
            if (!descriptor) return null;
            if (!descriptor.request)
                descriptor.request = new ReadableStreamBYOBRequest(this, descriptor);
            return descriptor.request;
        }
        enqueue(chunk) {
            const stream = this.__stream;
            if (stream.__state !== 'readable' || stream.__closeRequested)
                throw new TypeError('Readable byte stream is not accepting chunks');
            if (!validView(chunk)) throw new TypeError('Byte stream chunks must be nonempty ArrayBuffer views');
            if (stream.__byobReads.length)
                assertAttached(stream.__byobReads[0].buffer);
            const offset = chunk.byteOffset, length = chunk.byteLength;
            const bytes = new Uint8Array(transferred(chunk.buffer), offset, length);
            const pending = stream.__byobReads[0];
            if (pending?.orphan) {
                stream.__byobReads.shift();
                pending.request?.__invalidate();
                const carry = pending.filled ? new Uint8Array(new Uint8Array(
                    pending.buffer, pending.offset, pending.filled)) : null;
                for (const value of carry ? [carry, bytes] : [bytes]) {
                    if (stream.__reads.length)
                        stream.__reads.shift().resolve({ value, done: false });
                    else {
                        stream.__queue.push({ value, size: value.byteLength });
                        stream.__queueSize += value.byteLength;
                    }
                }
                if (stream.__byobReads.length) fill(stream, stream.__byobReads[0]);
            } else if (pending?.auto) {
                stream.__byobReads.shift();
                pending.request?.__invalidate();
                stream.__reads.shift()?.resolve({ value: bytes, done: false });
            } else if (stream.__byobReads.length) {
                const descriptor = stream.__byobReads[0];
                descriptor.buffer = transferred(descriptor.buffer);
                descriptor.request?.__invalidate();
                stream.__queue.push({ value: bytes, size: bytes.byteLength });
                stream.__queueSize += bytes.byteLength;
                fill(stream, stream.__byobReads[0]);
            } else if (stream.__reads.length) {
                stream.__reads.shift().resolve({ value: bytes, done: false });
            } else {
                stream.__queue.push({ value: bytes, size: bytes.byteLength });
                stream.__queueSize += bytes.byteLength;
            }
            stream.__pullIfNeeded();
        }
        close() {
            const stream = this.__stream;
            if (stream.__state !== 'readable' || stream.__closeRequested)
                throw new TypeError('Readable byte stream cannot be closed');
            const pending = stream.__byobReads[0];
            if (pending && pending.filled % pending.elementSize && !stream.__queue.length) {
                const error = new TypeError('Incomplete BYOB element at end of stream');
                stream.__finishError(error);
                throw error;
            }
            stream.__closeRequested = true;
            if (stream.__byobReads.length) fill(stream, stream.__byobReads[0]);
            if (!stream.__queue.length && !stream.__byobReads.length) stream.__finishClose();
        }
        error(reason) { this.__stream.__finishError(reason); }
    }

    class ReadableStreamBYOBReader {
        constructor(stream) {
            if (!(stream instanceof ReadableStream) || !stream.__byteStream || stream.locked)
                throw new TypeError('BYOB reader requires an unlocked readable byte stream');
            this.__stream = stream;
            stream.__reader = this;
        }
        get closed() {
            return this.__stream ? this.__stream.__closedPromise
                : this.__releasedClosed;
        }
        read(view, options = {}) {
            const stream = this.__stream;
            if (!stream) return reject('Reader was released');
            if (!validView(view)) return reject('BYOB read requires a nonempty ArrayBuffer view');
            const elementSize = view instanceof DataView ? 1 : view.BYTES_PER_ELEMENT;
            const capacity = view.byteLength / elementSize;
            const min = options?.min === undefined ? 1 : Number(options.min);
            if (!Number.isSafeInteger(min) || min < 1)
                return reject('Invalid minimum BYOB element count');
            if (min > capacity)
                return Promise.reject(new RangeError('Minimum exceeds the view length'));
            stream.__disturb();
            if (stream.__state === 'errored') return Promise.reject(stream.__storedError);
            if (stream.__state === 'closed') {
                const descriptor = {
                    type: view.constructor, elementSize, offset: view.byteOffset,
                    buffer: transferred(view.buffer)
                };
                return Promise.resolve({ value: resultView(descriptor, 0), done: true });
            }
            return makeDescriptor(stream, view, min).promise;
        }
        cancel(reason) {
            return this.__stream ? this.__stream.__cancel(reason) : reject('Reader was released');
        }
        releaseLock() {
            const stream = this.__stream;
            if (!stream) return;
            const error = new TypeError('Reader was released');
            for (const descriptor of stream.__byobReads) {
                if (!descriptor.orphan) {
                    descriptor.orphan = true;
                    descriptor.promise.catch(() => {});
                    descriptor.reject(error);
                }
            }
            if (stream.__state === 'readable') stream.__resetClosedPromise(error);
            this.__releasedClosed = Promise.reject(error);
            this.__releasedClosed.catch(() => {});
            stream.__reader = null; this.__stream = null;
        }
    }

    globalThis.__installReadableByteStreams({
        Reader: ReadableStreamBYOBReader,
        tee: globalThis.__byteTee,
        createController: (stream, source) => new ReadableByteStreamController(stream, source),
        releaseDefaultReader(stream) {
            for (const descriptor of stream.__byobReads)
                if (descriptor.auto) descriptor.orphan = true;
        },
        closeReads(stream) {
            for (const descriptor of stream.__byobReads.splice(0)) {
                descriptor.request?.__invalidate();
                if (descriptor.filled % descriptor.elementSize)
                    descriptor.reject(new TypeError('Incomplete BYOB element at end of stream'));
                else descriptor.resolve({ value: resultView(descriptor, descriptor.filled),
                    done: descriptor.filled === 0 });
            }
        },
        errorReads(stream, reason) {
            for (const descriptor of stream.__byobReads.splice(0)) {
                descriptor.request?.__invalidate();
                descriptor.reject(reason);
            }
        },
        cancelReads(stream) {
            for (const descriptor of stream.__byobReads.splice(0)) {
                descriptor.request?.__invalidate();
                if (descriptor.auto) descriptor.promise.catch(() => {});
                descriptor.resolve({ value: undefined, done: true });
            }
        }
    });
    delete globalThis.__installReadableByteStreams;
    delete globalThis.__byteTee;
    Object.assign(globalThis, {
        ReadableByteStreamController, ReadableStreamBYOBReader, ReadableStreamBYOBRequest
    });
})();
