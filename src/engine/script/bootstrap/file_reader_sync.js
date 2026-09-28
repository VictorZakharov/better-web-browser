// File API synchronous reads are worker-only. All bytes come from the private
// immutable Blob snapshot, not author-overridden Blob methods or properties.
(() => {
    'use strict';
    const [snapshot, concatBytes, bytesToBase64] = globalThis.__fileReaderSnapshot;
    const host = __hostCall;
    const readers = new WeakSet();
    const state = (reader, blob) => {
        if (!readers.has(reader)) throw new TypeError('Invalid FileReaderSync receiver');
        return snapshot(blob);
    };
    const binaryString = bytes => {
        let result = '';
        for (let offset = 0; offset < bytes.length; offset += 0x4000)
            result += String.fromCharCode(...bytes.subarray(offset, offset + 0x4000));
        return result;
    };
    const charsetOf = type => {
        const match = /(?:^|;)\s*charset\s*=\s*(?:"([^"]*)"|([^;\s]*))/i.exec(type);
        return match ? match[1] ?? match[2] : '';
    };

    class FileReaderSync {
        constructor() { readers.add(this); }
        readAsArrayBuffer(blob) {
            return concatBytes(state(this, blob).chunks).buffer;
        }
        readAsText(blob, encoding = undefined) {
            const source = state(this, blob);
            if (typeof encoding === 'symbol') throw new TypeError('Encoding must be a DOMString');
            return host('fileReadText', concatBytes(source.chunks),
                encoding === undefined ? '' : String(encoding), charsetOf(source.type));
        }
        readAsDataURL(blob) {
            const source = state(this, blob);
            return 'data:' + source.type + ';base64,' +
                bytesToBase64(concatBytes(source.chunks));
        }
        readAsBinaryString(blob) {
            return binaryString(concatBytes(state(this, blob).chunks));
        }
    }
    Object.defineProperty(FileReaderSync.prototype, Symbol.toStringTag,
        {configurable: true, value: 'FileReaderSync'});
    globalThis.FileReaderSync = FileReaderSync;
})();
