(() => {
    'use strict';
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
                if (part instanceof Blob) chunks.push(part.__bytes);
                else chunks.push(copyBytes(part) || encoder.encode(String(part)));
            }
            this.__bytes = concatBytes(chunks);
            const type = String(options?.type || '').toLowerCase();
            this.__type = /^[\x20-\x7e]*$/.test(type) ? type : '';
        }
        get size() { return this.__bytes.length; }
        get type() { return this.__type; }
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
    class File extends Blob {
        constructor(parts, name, options = {}) {
            super(parts, options);
            this.__name = String(name).replace(/\//g, ':');
            this.__lastModified = options.lastModified === undefined ? Date.now() : Number(options.lastModified);
        }
        get name() { return this.__name; }
        get lastModified() { return this.__lastModified; }
        get webkitRelativePath() { return ''; }
    }

    const formControlType = control => String(control.type).toLowerCase() ||
        (control.localName === 'button' ? 'submit' : control.localName === 'input' ? 'text' : '');
    class FormData {
        constructor(form = undefined, submitter = undefined) {
            this.__entries = [];
            if (form === undefined) return;
            if (!(form instanceof HTMLFormElement)) throw new TypeError('FormData requires an HTMLFormElement');
            if (submitter !== undefined && (submitter.form !== form ||
                !['submit', 'image'].includes(formControlType(submitter))))
                throw new DOMException('The submitter does not belong to this form', 'NotFoundError');
            for (const control of form.elements) {
                const name = control.name;
                const type = formControlType(control);
                if (!name || control.disabled || ['button', 'reset'].includes(type)) continue;
                if (['submit', 'image'].includes(type) && control !== submitter) continue;
                if (['checkbox', 'radio'].includes(type) && !control.checked) continue;
                if (control.localName === 'select') {
                    let options = control.querySelectorAll('option').filter(option => option.hasAttribute('selected'));
                    if (!control.multiple && !options.length) options = control.querySelectorAll('option').slice(0, 1);
                    for (const option of options) this.append(name,
                        option.hasAttribute('value') ? option.getAttribute('value') : option.textContent);
                } else if (type === 'file') this.append(name, new File([], ''));
                else this.append(name, control.value);
                if (control.dirName) this.append(control.dirName, 'ltr');
            }
        }
        append(name, value, filename = undefined) {
            name = String(name);
            if (value instanceof Blob && !(value instanceof File))
                value = new File([value], filename === undefined ? 'blob' : filename, { type: value.type });
            else if (value instanceof File && filename !== undefined)
                value = new File([value], filename, { type: value.type, lastModified: value.lastModified });
            else if (!(value instanceof Blob)) value = String(value);
            this.__entries.push([name, value]);
        }
        delete(name) { name = String(name); this.__entries = this.__entries.filter(entry => entry[0] !== name); }
        get(name) { return this.__entries.find(entry => entry[0] === String(name))?.[1] ?? null; }
        getAll(name) { return this.__entries.filter(entry => entry[0] === String(name)).map(entry => entry[1]); }
        has(name) { return this.__entries.some(entry => entry[0] === String(name)); }
        set(name, value, filename = undefined) {
            name = String(name);
            const index = this.__entries.findIndex(entry => entry[0] === name);
            if (index < 0) { this.append(name, value, filename); return; }
            const replacement = new FormData(); replacement.append(name, value, filename);
            this.__entries[index] = replacement.__entries[0];
            this.__entries = this.__entries.filter((entry, position) => position <= index || entry[0] !== name);
        }
        forEach(callback, thisArg = undefined) {
            if (typeof callback !== 'function') throw new TypeError('FormData callback must be callable');
            for (const [name, value] of this.__entries) callback.call(thisArg, value, name, this);
        }
        *entries() {
            for (let index = 0; index < this.__entries.length; index++) yield [...this.__entries[index]];
        }
        *keys() { for (const [name] of this.entries()) yield name; }
        *values() { for (const [, value] of this.entries()) yield value; }
        [Symbol.iterator]() { return this.entries(); }
    }

    const multipartBody = form => {
        const boundary = '----BreezeFormBoundary' + Math.floor(Math.random() * 0x1fffffffffffff).toString(16);
        const chunks = [];
        for (const [name, value] of form) {
            const escape = input => String(input).replace(/[\r\n"]/g, character => encodeURIComponent(character));
            let heading = '--' + boundary + '\r\nContent-Disposition: form-data; name="' + escape(name) + '"';
            if (value instanceof File) {
                heading += '; filename="' + escape(value.name) + '"\r\n';
                heading += 'Content-Type: ' + (value.type || 'application/octet-stream') + '\r\n\r\n';
                chunks.push(encoder.encode(heading), value.__bytes, encoder.encode('\r\n'));
            } else chunks.push(encoder.encode(heading + '\r\n\r\n' + value + '\r\n'));
        }
        chunks.push(encoder.encode('--' + boundary + '--\r\n'));
        return { bytes: concatBytes(chunks), stream: null, type: 'multipart/form-data; boundary=' + boundary };
    };
    const extractBody = body => {
        if (body == null) return { bytes: null, stream: null, type: '' };
        if (body instanceof ReadableStream) return { bytes: null, stream: body, type: '' };
        if (body instanceof Blob) return { bytes: new Uint8Array(body.__bytes), stream: null, type: body.type };
        if (body instanceof FormData) return multipartBody(body);
        if (body instanceof URLSearchParams)
            return { bytes: encoder.encode(body.toString()), stream: null, type: 'application/x-www-form-urlencoded;charset=UTF-8' };
        const bytes = copyBytes(body);
        if (bytes) return { bytes, stream: null, type: '' };
        return { bytes: encoder.encode(String(body)), stream: null, type: 'text/plain;charset=UTF-8' };
    };

    Object.assign(globalThis, { Headers, Blob, File, FormData });
    Object.defineProperty(globalThis, '__networkData', {
        configurable: true,
        value: Object.freeze({ concatBytes, bytesToBase64, extractBody, encoder, decoder })
    });
})();
