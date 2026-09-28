// File API byte-sequence algorithms. Blob chunks are private immutable snapshots;
// slicing can share their backing stores, while stream consumers receive copies.
(() => {
    'use strict';
    const STREAM_CHUNK_SIZE = 64 * 1024;

    // [Clamp] long long: round half to even after ToNumber, then bound to a
    // Blob's actual byte length. The bound also avoids imprecise huge offsets.
    const offset = (input, size) => {
        const number = +input;
        if (Number.isNaN(number)) return 0;
        if (number === Infinity) return size;
        if (number === -Infinity) return 0;
        const floor = Math.floor(number), fraction = number - floor;
        const rounded = fraction > 0.5 || (fraction === 0.5 && floor % 2 !== 0)
            ? floor + 1 : floor;
        return rounded < 0 ? Math.max(size + rounded, 0) : Math.min(rounded, size);
    };

    const slice = (chunks, size, start, end) => {
        const first = offset(start === undefined ? 0 : start, size);
        const last = offset(end === undefined ? size : end, size);
        const span = Math.max(last - first, 0);
        if (!span) return [];
        let position = 0;
        const result = [];
        for (const chunk of chunks) {
            const chunkEnd = position + chunk.byteLength;
            if (chunkEnd > first && position < last) {
                result.push(chunk.subarray(Math.max(first - position, 0),
                    Math.min(last - position, chunk.byteLength)));
            }
            position = chunkEnd;
            if (position >= last) break;
        }
        return result;
    };

    const stream = inputChunks => {
        let chunks = inputChunks, index = 0, position = 0;
        const next = limit => {
            while (index < chunks.length && position === chunks[index].byteLength) {
                ++index; position = 0;
            }
            if (index === chunks.length) return null;
            const chunk = chunks[index];
            const end = Math.min(position + limit, chunk.byteLength);
            const bytes = chunk.subarray(position, end);
            position = end;
            return bytes;
        };
        const exhausted = () => {
            while (index < chunks.length && position === chunks[index].byteLength) {
                ++index; position = 0;
            }
            return index === chunks.length;
        };
        return new ReadableStream({
            type: 'bytes',
            pull(controller) {
                const request = controller.byobRequest;
                let written = 0;
                if (request) {
                    const view = request.view;
                    while (written < view.byteLength) {
                        const bytes = next(view.byteLength - written);
                        if (!bytes) break;
                        view.set(bytes, written);
                        written += bytes.byteLength;
                    }
                    if (written) request.respond(written);
                } else {
                    const bytes = next(STREAM_CHUNK_SIZE);
                    if (bytes) controller.enqueue(new Uint8Array(bytes));
                }
                if (exhausted()) {
                    controller.close();
                    // A pending empty BYOB read settles only after close plus a
                    // zero-byte response; close alone leaves its descriptor live.
                    if (request && !written) request.respond(0);
                }
            },
            cancel() { chunks = []; index = position = 0; }
        });
    };

    Object.defineProperty(globalThis, '__blobByteAlgorithms', {
        configurable: true, value: Object.freeze({slice, stream})
    });
})();
