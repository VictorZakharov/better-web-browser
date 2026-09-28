// File API read operation and abort algorithms: https://w3c.github.io/FileAPI/
(() => {
    'use strict';
    const [snapshot, concatBytes, bytesToBase64] = globalThis.__fileReaderSnapshot;
    delete globalThis.__fileReaderSnapshot;
    const charsetOf = globalThis.__fileReaderMimeCharset;
    delete globalThis.__fileReaderMimeCharset;
    const schedule = globalThis.setTimeout, cancel = globalThis.clearTimeout;
    const now = Date.now.bind(Date), READ_CHUNK_SIZE = 64 * 1024;
    const trusted = globalThis.__markTrustedEvent, Progress = globalThis.ProgressEvent;
    const host = __hostCall, Exception = globalThis.DOMException;
    const dispatch = EventTarget.prototype.dispatchEvent;
    const add = EventTarget.prototype.addEventListener, remove = EventTarget.prototype.removeEventListener;
    const states = new WeakMap();
    const stateOf = reader => {
        const state = states.get(reader);
        if (!state) throw new TypeError('Invalid FileReader receiver');
        return state;
    };
    const string = value => {
        if (typeof value === 'symbol') throw new TypeError('Encoding must be a DOMString');
        return String(value);
    };
    const decode = (bytes, type, label) => {
        return host('fileReadText', bytes, label ?? '', charsetOf(type));
    };
    const packageData = (bytes, mode, type, label) => {
        if (mode === 'buffer') return bytes.buffer;
        if (mode === 'text') return decode(bytes, type, label);
        if (mode === 'url') return 'data:' + type + ';base64,' + bytesToBase64(bytes);
        let result = '';
        for (let offset = 0; offset < bytes.length; offset += 0x4000)
            result += String.fromCharCode(...bytes.subarray(offset, offset + 0x4000));
        return result;
    };
    const fire = (reader, type, loaded, total) => dispatch.call(reader, trusted(new Progress(type, {
        lengthComputable: true, loaded, total
    })));
    const read = (reader, blob, mode, label) => {
        const state = stateOf(reader), data = snapshot(blob);
        if (label !== undefined) label = string(label);
        if (state.readyState === 1) throw new Exception('A read is already in progress', 'InvalidStateError');
        state.readyState = 1; state.result = state.error = null;
        state.loaded = 0; state.total = data.size;
        const generation = ++state.generation;
        const current = () => generation === state.generation && state.readyState === 1;
        state.task = schedule(() => {
            if (!current()) return;
            fire(reader, 'loadstart', 0, data.size);
            if (!current()) return;
            let chunkIndex = 0, chunkOffset = 0, lastProgress = now();
            const pump = () => {
                if (!current()) return;
                // Yield between bounded byte ranges so long reads do not monopolize
                // the document or worker event loop. The underlying chunks are
                // private snapshots and remain immutable across these tasks.
                let budget = READ_CHUNK_SIZE;
                while (budget && chunkIndex < data.chunks.length) {
                    const chunk = data.chunks[chunkIndex];
                    const amount = Math.min(chunk.length - chunkOffset, budget);
                    if (amount) {
                        chunkOffset += amount;
                        budget -= amount;
                        state.loaded += amount;
                    }
                    if (chunkOffset === chunk.length) {
                        ++chunkIndex; chunkOffset = 0;
                    }
                }
                const complete = chunkIndex === data.chunks.length;
                const elapsed = now() - lastProgress;
                if (state.loaded > 0 && elapsed >= 50) {
                    lastProgress = now();
                    fire(reader, 'progress', state.loaded, data.size);
                    if (!current()) return;
                }
                if (!complete) {
                    state.task = schedule(pump, 0);
                    return;
                }
                let result, error;
                try { result = packageData(concatBytes(data.chunks), mode, data.type, label); }
                catch (_) { error = new Exception('Could not read Blob bytes', 'NotReadableError'); }
                state.readyState = 2; state.task = null;
                state.result = error ? null : result; state.error = error || null;
                fire(reader, error ? 'error' : 'load', state.loaded, data.size);
                // A load/error handler may start a new read. Its state must not be
                // overwritten, nor may the previous operation emit loadend.
                if (state.readyState !== 1) fire(reader, 'loadend', state.loaded, data.size);
            };
            state.task = schedule(pump, 0);
        }, 0);
    };
    class FileReader extends EventTarget {
        constructor() {
            super();
            states.set(this, {readyState:0, result:null, error:null, generation:0,
                task:null, loaded:0, total:0, handlers:new Map()});
        }
        get readyState() { return stateOf(this).readyState; }
        get result() { return stateOf(this).result; }
        get error() { return stateOf(this).error; }
        readAsArrayBuffer(blob) { read(this, blob, 'buffer'); }
        readAsText(blob, encoding) { read(this, blob, 'text', encoding); }
        readAsDataURL(blob) { read(this, blob, 'url'); }
        readAsBinaryString(blob) { read(this, blob, 'binary'); }
        abort() {
            const state = stateOf(this);
            state.result = null;
            if (state.readyState !== 1) return;
            ++state.generation;
            cancel(state.task); state.task = null; state.readyState = 2;
            fire(this, 'abort', state.loaded, state.total);
            if (state.readyState !== 1) fire(this, 'loadend', state.loaded, state.total);
        }
    }
    for (const [name, value] of [['EMPTY',0], ['LOADING',1], ['DONE',2]])
        for (const target of [FileReader, FileReader.prototype])
            Object.defineProperty(target, name, {enumerable:true, value});
    for (const type of ['loadstart', 'progress', 'load', 'error', 'abort', 'loadend'])
        Object.defineProperty(FileReader.prototype, 'on' + type, {
            configurable:true, enumerable:true,
            get() { return stateOf(this).handlers.get(type)?.callback ?? null; },
            set(value) {
                const handlers = stateOf(this).handlers, previous = handlers.get(type);
                const callback = typeof value === 'function' ? value : null;
                if (previous && callback) { previous.callback = callback; return; }
                if (previous) { remove.call(this, type, previous.listener); handlers.delete(type); }
                if (callback) {
                    const item = {callback, listener:event => item.callback.call(this, event)};
                    handlers.set(type, item); add.call(this, type, item.listener);
                }
            }
        });
    Object.defineProperty(FileReader.prototype, Symbol.toStringTag, {configurable:true, value:'FileReader'});
    globalThis.FileReader = FileReader;
})();
