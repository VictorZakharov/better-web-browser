(() => {
    'use strict';
    const formats = new Set(['gzip', 'deflate', 'deflate-raw']);

    const inputBytes = chunk => {
        if (chunk instanceof ArrayBuffer) return new Uint8Array(chunk);
        if (ArrayBuffer.isView(chunk))
            return new Uint8Array(chunk.buffer, chunk.byteOffset, chunk.byteLength);
        throw new TypeError('Compression stream chunks must be BufferSource values');
    };

    class CompressionTransform {
        constructor(mode, format) {
            if (arguments.length < 2 || !formats.has(String(format)))
                throw new TypeError('Unsupported compression format');
            const id = __hostCall('compressionCreate', mode, String(format));
            let active = true;
            const release = () => {
                if (active) { active = false; __hostCall('compressionDrop', id); }
            };
            const transform = new TransformStream({
                transform(chunk, controller) {
                    try {
                        const bytes = __hostCall('compressionWrite', id, inputBytes(chunk));
                        if (bytes.byteLength) controller.enqueue(bytes);
                    } catch (error) { release(); throw error; }
                },
                flush(controller) {
                    try {
                        const bytes = __hostCall('compressionFinish', id);
                        active = false;
                        if (bytes.byteLength) controller.enqueue(bytes);
                    } catch (error) { release(); throw error; }
                },
                cancel: release
            });
            this.readable = transform.readable;
            this.writable = transform.writable;
        }
    }

    class CompressionStream extends CompressionTransform {
        constructor(format) { super('compress', format); }
    }
    class DecompressionStream extends CompressionTransform {
        constructor(format) { super('decompress', format); }
    }

    Object.assign(globalThis, { CompressionStream, DecompressionStream });
})();
