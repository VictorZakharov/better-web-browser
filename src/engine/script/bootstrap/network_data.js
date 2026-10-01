(() => {
    'use strict';
    const FormData = globalThis.FormData;
    // FormData's entry list is an internal slot, not the author-overridable
    // iterator or a writable property on the instance. Capture its private
    // accessor before any page script runs (also in the worker bootstrap).
    const formDataEntrySnapshot = globalThis.__formDataEntrySnapshot;
    delete globalThis.__formDataEntrySnapshot;
    const urlApi = globalThis.__urlInternals;
    const blobBytes = globalThis.__blobByteAlgorithms;
    delete globalThis.__blobByteAlgorithms;
    const encoder = new TextEncoder();
    const decoder = new TextDecoder();
    const headerNamePattern = /^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/;
    const forbiddenRequestNames = new Set([
        'accept-charset', 'accept-encoding', 'access-control-request-headers',
        'access-control-request-method', 'connection', 'content-length', 'cookie', 'cookie2',
        'date', 'dnt', 'expect', 'host', 'keep-alive', 'origin', 'permissions-policy',
        'proxy-authenticate', 'proxy-authorization', 'referer', 'set-cookie', 'te', 'trailer',
        'transfer-encoding', 'upgrade', 'via'
    ]);
    const forbiddenMethodHeaders = new Set(['x-http-method', 'x-http-method-override', 'x-method-override']);
    const noCorsNames = new Set(['accept', 'accept-language', 'content-language', 'content-type', 'range']);
    const iteratorPrototype = Object.getPrototypeOf(Object.getPrototypeOf([][Symbol.iterator]()));
    const headersIteratorPrototype = Object.create(iteratorPrototype);
    Object.defineProperties(headersIteratorPrototype, {
        next: {
            configurable: true, enumerable: true, writable: true,
            value() {
                const entry = this.__headers?.__sortedAndCombined()[this.__index++];
                if (!entry) return { value: undefined, done: true };
                if (this.__kind === 'key') return { value: entry[0], done: false };
                if (this.__kind === 'value') return { value: entry[1], done: false };
                return { value: [...entry], done: false };
            }
        },
        [Symbol.toStringTag]: { configurable: true, value: 'Headers Iterator' }
    });
    const headersIterator = (headers, kind) => {
        const iterator = Object.create(headersIteratorPrototype);
        iterator.__headers = headers; iterator.__kind = kind; iterator.__index = 0;
        return iterator;
    };

    const toByteString = (value, description) => {
        if (typeof value === 'symbol') throw new TypeError(description + ' is not a ByteString');
        const string = String(value);
        for (let index = 0; index < string.length; index++)
            if (string.charCodeAt(index) > 255) throw new TypeError(description + ' is not a ByteString');
        return string;
    };
    const normalizeName = value => {
        const name = toByteString(value, 'Header name');
        const normalized = name.toLowerCase();
        if (!headerNamePattern.test(normalized)) throw new TypeError('Invalid HTTP header name');
        return normalized;
    };
    const normalizeValue = value => {
        value = toByteString(value, 'Header value');
        value = value.replace(/^[\t\n\r ]+|[\t\n\r ]+$/g, '');
        if (/[\0\r\n]/.test(value)) throw new TypeError('Invalid HTTP header value');
        return value;
    };
    const forbiddenRequestHeader = (name, value) => forbiddenRequestNames.has(name) ||
        name.startsWith('proxy-') || name.startsWith('sec-') ||
        forbiddenMethodHeaders.has(name) && value.split(',').some(method =>
            ['CONNECT', 'TRACE', 'TRACK'].includes(method.trim().toUpperCase()));
    const corsUnsafeByte = code => code < 0x20 && code !== 0x09 || code === 0x7f ||
        '"():<>?@[\\]{}'.includes(String.fromCharCode(code));
    const noCorsSafelisted = (name, value) => {
        if (value.length > 128) return false;
        if (name === 'accept') return ![...value].some(character => corsUnsafeByte(character.charCodeAt(0)));
        if (name === 'accept-language' || name === 'content-language')
            return /^[0-9A-Za-z *,\-.;=]*$/.test(value);
        if (name === 'content-type') {
            const essence = value.split(';', 1)[0].trim().toLowerCase();
            return ['application/x-www-form-urlencoded', 'multipart/form-data', 'text/plain'].includes(essence) &&
                ![...value].some(character => corsUnsafeByte(character.charCodeAt(0)));
        }
        if (name === 'range') {
            const match = /^bytes=([0-9]+)-([0-9]*)$/.exec(value);
            return !!match && (match[2] === '' || Number(match[2]) >= Number(match[1]));
        }
        return false;
    };

    class Headers {
        constructor(init = undefined) {
            this.__entries = [];
            this.__guard = 'none';
            if (init !== undefined) this.__fill(init);
        }
        __fill(init) {
            if ((typeof init !== 'object' && typeof init !== 'function') || init === null)
                throw new TypeError('Headers initializer must be a sequence or record');
            const iterator = init[Symbol.iterator];
            if (iterator !== undefined) {
                if (typeof iterator !== 'function') throw new TypeError('Headers iterator is not callable');
                for (const pair of init) {
                    if ((typeof pair !== 'object' && typeof pair !== 'function') || pair === null)
                        throw new TypeError('Header pair must be a sequence');
                    const values = [...pair];
                    if (values.length !== 2) throw new TypeError('Header pair must contain two items');
                    this.append(values[0], values[1]);
                }
                return;
            }
            // Web IDL record conversion is deliberately spelled out here. In
            // addition to preserving key order, each step is observable to a
            // Proxy and key conversion must precede the corresponding [[Get]].
            // Reflect.ownKeys also enforces the Proxy invariant against duplicate
            // keys, which Object.keys() does not expose with the required order.
            for (const key of Reflect.ownKeys(init)) {
                const descriptor = Reflect.getOwnPropertyDescriptor(init, key);
                if (!descriptor || !descriptor.enumerable) continue;
                const name = toByteString(key, 'Header name');
                const value = toByteString(Reflect.get(init, key), 'Header value');
                this.append(name, value);
            }
        }
        __setGuard(guard) { this.__guard = guard; return this; }
        __allows(name, value, operation) {
            if (this.__guard === 'immutable') throw new TypeError('Headers are immutable');
            if (this.__guard === 'request' || this.__guard === 'request-no-cors') {
                if (forbiddenRequestHeader(name, value)) return false;
            }
            if (this.__guard === 'request-no-cors') {
                if (operation === 'delete') return noCorsNames.has(name);
                const current = this.get(name);
                const proposed = operation === 'append' && current !== null ? current + ', ' + value : value;
                return noCorsSafelisted(name, proposed);
            }
            if (this.__guard === 'response' && (name === 'set-cookie' || name === 'set-cookie2')) return false;
            return true;
        }
        append(name, value) {
            name = normalizeName(name); value = normalizeValue(value);
            if (this.__allows(name, value, 'append')) this.__entries.push([name, value]);
        }
        delete(name) {
            name = normalizeName(name);
            if (this.__allows(name, '', 'delete'))
                this.__entries = this.__entries.filter(entry => entry[0] !== name);
        }
        get(name) {
            name = normalizeName(name);
            const values = this.__entries.filter(entry => entry[0] === name).map(entry => entry[1]);
            return values.length ? values.join(', ') : null;
        }
        getSetCookie() { return this.__entries.filter(entry => entry[0] === 'set-cookie').map(entry => entry[1]); }
        has(name) { name = normalizeName(name); return this.__entries.some(entry => entry[0] === name); }
        set(name, value) {
            name = normalizeName(name); value = normalizeValue(value);
            if (!this.__allows(name, value, 'set')) return;
            this.__entries = this.__entries.filter(entry => entry[0] !== name);
            this.__entries.push([name, value]);
        }
        __sortedAndCombined() {
            const names = [...new Set(this.__entries.map(entry => entry[0]))].sort();
            const output = [];
            for (const name of names) {
                const values = this.__entries.filter(entry => entry[0] === name).map(entry => entry[1]);
                if (name === 'set-cookie') for (const value of values) output.push([name, value]);
                else output.push([name, values.join(', ')]);
            }
            return output;
        }
        forEach(callback, thisArg = undefined) {
            if (typeof callback !== 'function') throw new TypeError('Headers callback must be callable');
            for (const [name, value] of this) callback.call(thisArg, value, name, this);
        }
        entries() { return headersIterator(this, 'entry'); }
        keys() { return headersIterator(this, 'key'); }
        values() { return headersIterator(this, 'value'); }
        [Symbol.iterator]() { return this.entries(); }
    }

    const copyBytes = value => {
        if (value instanceof ArrayBuffer) return new Uint8Array(value.slice(0));
        if (ArrayBuffer.isView?.(value))
            return new Uint8Array(value.buffer.slice(value.byteOffset, value.byteOffset + value.byteLength));
        return null;
    };
    const concatBytes = chunks => {
        const size = chunks.reduce((total, chunk) => total + chunk.length, 0);
        const output = new Uint8Array(size);
        let offset = 0;
        for (const chunk of chunks) { output.set(chunk, offset); offset += chunk.length; }
        return output;
    };
    const normalizedBlobType = value => {
        if (typeof value === 'symbol') throw new TypeError('Blob type must be a DOMString');
        const type = (value === undefined ? '' : String(value))
            .replace(/[A-Z]/g, character => character.toLowerCase());
        return [...type].every(character => {
            const code = character.charCodeAt(0);
            return code >= 0x20 && code <= 0x7e;
        }) ? type : '';
    };
    const blobStates = new WeakMap(), fileStates = new WeakMap();
    const blobState = blob => {
        const state = blobStates.get(blob);
        if (!state) throw new TypeError('Invalid Blob receiver');
        return state;
    };
    const materializeBlob = blob => {
        const state = blobState(blob);
        if (state.chunks.length > 1) state.chunks = [concatBytes(state.chunks)];
        return state.chunks[0];
    };
    // Window Canvas is initialized earlier; worker Canvas is initialized later.
    // Pass immutable bytes through a bootstrap-only capability, then remove it.
    // createImageBitmap must not call author-overridable Blob.bytes()/getters.
    const imageBitmapBlobSnapshot = value => {
        const state = blobStates.get(value);
        if (!state) return null;
        if (state.size > 24 * 1024 * 1024)
            throw new DOMException('Image source exceeds the decode budget', 'InvalidStateError');
        return new Uint8Array(materializeBlob(value));
    };
    if (globalThis.__bindImageBitmapBlob) {
        globalThis.__bindImageBitmapBlob(imageBitmapBlobSnapshot);
        delete globalThis.__bindImageBitmapBlob;
    } else globalThis.__imageBitmapBlobSnapshot = imageBitmapBlobSnapshot;
    const initializeBlob = (blob, chunks, type) => {
        blobStates.set(blob, {chunks: chunks.length ? chunks : [new Uint8Array()],
            size: chunks.reduce((total, chunk) => total + chunk.length, 0), type: normalizedBlobType(type)});
        return blob;
    };
    const bytesToBase64 = bytes => {
        let binary = '';
        for (let start = 0; start < bytes.length; start += 0x4000)
            binary += String.fromCharCode(...bytes.subarray(start, start + 0x4000));
        return btoa(binary);
    };

    class Blob {
        constructor(parts = [], options = {}) {
            const chunks = [];
            for (const part of parts) {
                // Blob byte sequences are immutable, so another Blob's owned chunks can be
                // shared without changing the observable snapshot.
                if (blobStates.has(part)) chunks.push(...blobState(part).chunks);
                else chunks.push(copyBytes(part) || encoder.encode(String(part)));
            }
            initializeBlob(this, chunks, options?.type);
        }
        __materializeBytes() {
            return new Uint8Array(materializeBlob(this));
        }
        get __chunks() { return blobState(this).chunks.map(chunk => new Uint8Array(chunk)); }
        get __bytes() { return new Uint8Array(materializeBlob(this)); }
        get size() { return blobState(this).size; }
        get type() { return blobState(this).type; }
        slice(start = 0, end = undefined, type = '') {
            const state = blobState(this);
            const bytes = blobBytes.slice(state.chunks, state.size, start, end);
            return initializeBlob(Object.create(Blob.prototype), bytes, type);
        }
        arrayBuffer() { return Promise.resolve(new Uint8Array(materializeBlob(this)).buffer); }
        bytes() { return Promise.resolve(new Uint8Array(materializeBlob(this))); }
        stream() {
            return blobBytes.stream(blobState(this).chunks);
        }
        text() { return Promise.resolve(decoder.decode(materializeBlob(this))); }
    }
    // Network-delivered chunks are already private immutable snapshots. Adopting them avoids a
    // second full-body copy merely to expose Blob.size to an XHR load handler.
    const blobFromOwnedBytes = (chunks, type) =>
        initializeBlob(Object.create(Blob.prototype), chunks, type);
    Object.defineProperty(Blob, '__fromOwnedBytes', { value: blobFromOwnedBytes });
    class File extends Blob {
        constructor(parts, name, options = {}) {
            super(parts, options);
            fileStates.set(this, {name: String(name).replace(/\//g, ':'),
                lastModified: options.lastModified === undefined ? Date.now() : Number(options.lastModified)});
        }
        get name() { return fileStates.get(this).name; }
        get lastModified() { return fileStates.get(this).lastModified; }
        get webkitRelativePath() { return ''; }
    }

    // The picker delivers already owned bytes. Keep the factory private so
    // author overrides of File and Blob cannot substitute the selected snapshot.
    if (typeof document !== 'undefined') {
        globalThis.__installFilePickerFileFactory?.((bytes, name, type, lastModified) => {
            const file = initializeBlob(Object.create(File.prototype), [bytes], type);
            fileStates.set(file, { name, lastModified });
            return file;
        });
        delete globalThis.__installFilePickerFileFactory;
    }


    const multipartBody = (form, maximumBytes = Infinity) => {
        const boundary = '----BreezeFormBoundary' + Math.floor(Math.random() * 0x1fffffffffffff).toString(16);
        const chunks = [];
        let total = 0;
        const add = chunk => {
            if (chunk.length > maximumBytes - total)
                throw new RangeError('Form body exceeds the browser limit');
            total += chunk.length;
            chunks.push(chunk);
        };
        for (const [name, value] of formDataEntrySnapshot(form)) {
            const normalize = input => String(input).replace(/\r\n|\r|\n/g, '\r\n');
            const escape = input => normalize(input).replace(/[\r\n"]/g, character => encodeURIComponent(character));
            let heading = '--' + boundary + '\r\nContent-Disposition: form-data; name="' + escape(name) + '"';
            if (fileStates.has(value)) {
                const file = fileStates.get(value), blob = blobState(value);
                heading += '; filename="' + escape(file.name) + '"\r\n';
                heading += 'Content-Type: ' + (blob.type || 'application/octet-stream') + '\r\n\r\n';
                add(encoder.encode(heading));
                if (blob.size > maximumBytes - total)
                    throw new RangeError('Form body exceeds the browser limit');
                add(new Uint8Array(materializeBlob(value)));
                add(encoder.encode('\r\n'));
            } else add(encoder.encode(heading + '\r\n\r\n' + normalize(value) + '\r\n'));
        }
        add(encoder.encode('--' + boundary + '--\r\n'));
        return { bytes: concatBytes(chunks), stream: null, type: 'multipart/form-data; boundary=' + boundary };
    };
    const extractBody = (body, maximumBytes = Infinity) => {
        if (!(maximumBytes >= 0)) throw new RangeError('Invalid body size limit');
        const bounded = result => {
            if (result.bytes && result.bytes.length > maximumBytes)
                throw new RangeError('Body exceeds the browser limit');
            return result;
        };
        if (body == null) return { bytes: null, stream: null, type: '' };
        if (body instanceof ReadableStream) return { bytes: null, stream: body, type: '' };
        if (blobStates.has(body)) {
            const state = blobState(body);
            if (state.size > maximumBytes) throw new RangeError('Body exceeds the browser limit');
            return { bytes: new Uint8Array(materializeBlob(body)), stream: null, type: state.type };
        }
        // Window and dedicated workers share the same private FormData entry
        // list; author iterator overrides must not change serialized bytes.
        if (FormData && body instanceof FormData) return multipartBody(body, maximumBytes);
        if (urlApi.isParams(body))
            return bounded({ bytes: encoder.encode(urlApi.serializeParams(body)), stream: null,
                type: 'application/x-www-form-urlencoded;charset=UTF-8' });
        const bytes = copyBytes(body);
        if (bytes) return bounded({ bytes, stream: null, type: '' });
        return bounded({ bytes: encoder.encode(String(body)), stream: null, type: 'text/plain;charset=UTF-8' });
    };

    Object.assign(globalThis, { Headers, Blob, File });
    // FormData's Blob overload creates a File entry without consulting
    // author-overridable Blob.type/File.name getters or constructors. Install
    // the factory after publishing both constructors to this realm.
    globalThis.__installFormDataFileFactory?.((source, name) => {
        const blob = blobState(source);
        const originalFile = fileStates.get(source);
        const file = initializeBlob(Object.create(File.prototype), blob.chunks.slice(), blob.type);
        fileStates.set(file, { name: String(name).replace(/\//g, ':'),
            lastModified: originalFile?.lastModified ?? Date.now() });
        return file;
    });
    delete globalThis.__installFormDataFileFactory;
    // The form-submission bootstrap runs later in Window only and captures
    // this once; workers must not expose an internal entry-list accessor.
    if (typeof document !== 'undefined')
        globalThis.__internalFormDataEntries = formDataEntrySnapshot;
    // FileReader consumes a private immutable snapshot, not author-overridden
    // Blob methods or properties. The next bootstrap extension removes this hook.
    globalThis.__fileReaderSnapshot = [value => {
        const state = blobState(value);
        return { chunks: state.chunks, size: state.size, type: state.type };
    }, concatBytes, bytesToBase64];
    // Structured clone also needs the private snapshot in worker realms. Its
    // bootstrap captures then deletes this temporary capability before authors run.
    globalThis.__blobStructuredCloneSnapshot = value => {
        const state = blobStates.get(value);
        if (!state) return null;
        const file = fileStates.get(value);
        return { bytes: new Uint8Array(materializeBlob(value)), type: state.type,
            file: file ? {name: file.name, lastModified: file.lastModified} : null };
    };
    // Captured and removed before author execution. Native structured clone invokes these
    // closures, never mutable author getters, prototypes, or constructor properties.
    if (typeof document !== 'undefined') globalThis.__blobCloneBindings = [
        value => blobStates.has(value),
        value => {const state=blobState(value), file=fileStates.get(value);
            return [state.chunks,state.type,file?.name,file?.lastModified];},
        value => value[2] === undefined ? new Blob(value[0], {type:value[1]})
            : new File(value[0], value[2], {type:value[1],lastModified:value[3]})
    ];
    Object.defineProperty(globalThis, '__networkData', {
        configurable: true,
        value: Object.freeze({ concatBytes, bytesToBase64, extractBody, encoder, decoder,
            fileName: value => fileStates.get(value)?.name })
    });
})();
