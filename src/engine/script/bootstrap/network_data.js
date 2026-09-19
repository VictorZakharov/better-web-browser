(() => {
    'use strict';
    const FormData = globalThis.FormData;
    const urlApi = globalThis.__urlInternals;
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
        const type = String(value || '').toLowerCase();
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
        slice(start = 0, end = this.size, type = '') {
            const normalize = value => value < 0 ? Math.max(this.size + value, 0) : Math.min(value, this.size);
            start = normalize(Number(start) || 0);
            end = normalize(end === undefined ? this.size : Number(end) || 0);
            return new Blob([this.__bytes.slice(start, Math.max(start, end))], { type });
        }
        arrayBuffer() { return Promise.resolve(this.__bytes.buffer.slice(0)); }
        bytes() { return Promise.resolve(new Uint8Array(this.__bytes)); }
        stream() {
            const bytes = new Uint8Array(this.__bytes);
            return new ReadableStream({ start(controller) { controller.enqueue(bytes); controller.close(); } });
        }
        text() { return Promise.resolve(decoder.decode(this.__bytes)); }
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


    const multipartBody = form => {
        const boundary = '----BreezeFormBoundary' + Math.floor(Math.random() * 0x1fffffffffffff).toString(16);
        const chunks = [];
        for (const [name, value] of form) {
            const normalize = input => String(input).replace(/\r\n|\r|\n/g, '\r\n');
            const escape = input => normalize(input).replace(/[\r\n"]/g, character => encodeURIComponent(character));
            let heading = '--' + boundary + '\r\nContent-Disposition: form-data; name="' + escape(name) + '"';
            if (value instanceof File) {
                heading += '; filename="' + escape(value.name) + '"\r\n';
                heading += 'Content-Type: ' + (value.type || 'application/octet-stream') + '\r\n\r\n';
                chunks.push(encoder.encode(heading), value.__bytes, encoder.encode('\r\n'));
            } else chunks.push(encoder.encode(heading + '\r\n\r\n' + normalize(value) + '\r\n'));
        }
        chunks.push(encoder.encode('--' + boundary + '--\r\n'));
        return { bytes: concatBytes(chunks), stream: null, type: 'multipart/form-data; boundary=' + boundary };
    };
    const extractBody = body => {
        if (body == null) return { bytes: null, stream: null, type: '' };
        if (body instanceof ReadableStream) return { bytes: null, stream: body, type: '' };
        if (body instanceof Blob) return { bytes: new Uint8Array(body.__bytes), stream: null, type: body.type };
        if (body instanceof FormData) return multipartBody(body);
        if (urlApi.isParams(body))
            return { bytes: encoder.encode(urlApi.serializeParams(body)), stream: null, type: 'application/x-www-form-urlencoded;charset=UTF-8' };
        const bytes = copyBytes(body);
        if (bytes) return { bytes, stream: null, type: '' };
        return { bytes: encoder.encode(String(body)), stream: null, type: 'text/plain;charset=UTF-8' };
    };

    Object.assign(globalThis, { Headers, Blob, File });
    // FileReader consumes a private immutable snapshot, not author-overridden
    // Blob methods or properties. The next bootstrap extension removes this hook.
    globalThis.__fileReaderSnapshot = [value => {
        const state = blobState(value);
        return { chunks: state.chunks, size: state.size, type: state.type };
    }, concatBytes, bytesToBase64];
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
        value: Object.freeze({ concatBytes, bytesToBase64, extractBody, encoder, decoder })
    });
})();
