(() => {
    'use strict';

    // Encoding Standard: encodeInto reports UTF-16 code units consumed, not
    // code points. An incomplete scalar must not be partially written.
    const scalarAt = (input, index) => {
        const first = input.charCodeAt(index);
        if (first >= 0xD800 && first <= 0xDBFF && index + 1 < input.length) {
            const second = input.charCodeAt(index + 1);
            if (second >= 0xDC00 && second <= 0xDFFF)
                return [0x10000 + ((first - 0xD800) << 10) + second - 0xDC00, 2];
        }
        return [first >= 0xD800 && first <= 0xDFFF ? 0xFFFD : first, 1];
    };
    const utf8 = scalar => {
        if (scalar <= 0x7F) return [scalar];
        if (scalar <= 0x7FF) return [0xC0 | (scalar >> 6), 0x80 | (scalar & 0x3F)];
        if (scalar <= 0xFFFF) return [0xE0 | (scalar >> 12), 0x80 | ((scalar >> 6) & 63), 0x80 | (scalar & 63)];
        return [0xF0 | (scalar >> 18), 0x80 | ((scalar >> 12) & 63),
            0x80 | ((scalar >> 6) & 63), 0x80 | (scalar & 63)];
    };
    class TextEncoder {
        get encoding() { return 'utf-8'; }
        encode(input = '') {
            input = String(input);
            const output = [];
            for (let index = 0; index < input.length;) {
                const [scalar, units] = scalarAt(input, index);
                output.push(...utf8(scalar));
                index += units;
            }
            return new Uint8Array(output);
        }
        encodeInto(source, destination) {
            if (!(destination instanceof Uint8Array))
                throw new TypeError('destination must be a Uint8Array');
            source = String(source);
            let read = 0, written = 0;
            while (read < source.length) {
                const [scalar, units] = scalarAt(source, read);
                const bytes = utf8(scalar);
                if (written + bytes.length > destination.length) break;
                destination.set(bytes, written);
                written += bytes.length;
                read += units;
            }
            return { read, written };
        }
    }

    const inputView = input => {
        if (input === undefined) return new Uint8Array();
        if (input instanceof ArrayBuffer ||
            Object.prototype.toString.call(input) === '[object SharedArrayBuffer]')
            return new Uint8Array(input);
        if (ArrayBuffer.isView?.(input))
            return new Uint8Array(input.buffer, input.byteOffset, input.byteLength);
        throw new TypeError('input must be an ArrayBuffer or an ArrayBuffer view');
    };
    class TextDecoder {
        constructor(label = 'utf-8', options = {}) {
            this.__encoding = __hostCall('decoderLabel', String(label));
            this.__fatal = !!options.fatal;
            this.__ignoreBOM = !!options.ignoreBOM;
            this.__streamId = 0;
        }
        get encoding() { return this.__encoding; }
        get fatal() { return this.__fatal; }
        get ignoreBOM() { return this.__ignoreBOM; }
        decode(input, options = {}) {
            // BufferSource conversion precedes the options dictionary. A
            // stream getter may detach the buffer before its bytes are read.
            const bytes = inputView(input);
            const stream = !!options.stream;
            if (stream && !this.__streamId)
                this.__streamId = __hostCall('decoderCreate', this.__encoding, this.__ignoreBOM);
            try {
                const [text, id] = __hostCall('decoderDecode', this.__encoding,
                    bytes, stream, this.__fatal, this.__ignoreBOM, this.__streamId);
                this.__streamId = id;
                return text;
            } catch (error) {
                if (!stream) this.__streamId = 0;
                throw error;
            }
        }
    }
    Object.assign(globalThis, { TextEncoder, TextDecoder });
})();
