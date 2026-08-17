(() => {
    'use strict';
    const data = globalThis.__networkData;
    const streamForBytes = bytes => new ReadableStream({
        start(controller) { controller.enqueue(new Uint8Array(bytes)); controller.close(); }
    });
    const initializeBody = (target, extracted) => {
        target.__bodyBytes = extracted.bytes === null ? null : new Uint8Array(extracted.bytes);
        target.__bodyStream = extracted.stream ||
            (target.__bodyBytes === null ? null : streamForBytes(target.__bodyBytes));
    };
    const bodyUnusable = target => target.__bodyStream !== null &&
        (target.__bodyStream.locked || target.__bodyStream.__disturbed);
    const readBodyBytes = target => {
        if (bodyUnusable(target)) return Promise.reject(new TypeError('Body has already been consumed'));
        if (target.__bodyStream === null) return Promise.resolve(new Uint8Array());
        if (target.__bodyBytes !== null) {
            target.__bodyStream.__disturb();
            return Promise.resolve(new Uint8Array(target.__bodyBytes));
        }
        const reader = target.__bodyStream.getReader();
        const chunks = [];
        const read = () => reader.read().then(result => {
            if (result.done) { reader.releaseLock(); return data.concatBytes(chunks); }
            if (!(result.value instanceof Uint8Array)) {
                reader.releaseLock();
                throw new TypeError('Body stream chunks must be Uint8Array values');
            }
            chunks.push(new Uint8Array(result.value));
            return read();
        }, error => { try { reader.releaseLock(); } catch (_) {} throw error; });
        return read();
    };
    const cloneBody = source => {
        if (source.__bodyStream === null) return { bytes: null, stream: null, type: '' };
        if (source.__bodyBytes !== null)
            return { bytes: new Uint8Array(source.__bodyBytes), stream: null, type: '' };
        // Fetch cloning structurally clones chunks for the second tee branch.
        const [left, right] = source.__bodyStream.__tee(true);
        source.__bodyStream = left;
        return { bytes: null, stream: right, type: '' };
    };
    const transferBody = source => {
        if (source.__bodyStream === null) return { bytes: null, stream: null, type: '' };
        if (source.__bodyBytes !== null) {
            source.__bodyStream.__disturb();
            return { bytes: new Uint8Array(source.__bodyBytes), stream: null, type: '' };
        }
        const reader = source.__bodyStream.getReader();
        source.__bodyStream.__disturb();
        return {
            bytes: null,
            stream: new ReadableStream({
                pull(controller) {
                    return reader.read().then(({ value, done }) => {
                        if (done) { reader.releaseLock(); controller.close(); }
                        else controller.enqueue(value);
                    });
                },
                cancel(reason) { return reader.cancel(reason); }
            }),
            type: ''
        };
    };

    const mime = value => {
        const [essence, ...parameters] = String(value || '').split(';');
        const result = { essence: essence.trim().toLowerCase(), parameters: new Map() };
        for (const parameter of parameters) {
            const [name, raw] = parameter.split('=', 2);
            if (!raw) continue;
            result.parameters.set(name.trim().toLowerCase(), raw.trim().replace(/^"|"$/g, ''));
        }
        return result;
    };
    const binaryString = bytes => {
        let output = '';
        for (let start = 0; start < bytes.length; start += 0x4000)
            output += String.fromCharCode(...bytes.subarray(start, start + 0x4000));
        return output;
    };
    const binaryBytes = value => Uint8Array.from(value, character => character.charCodeAt(0));
    const multipartFormData = (bytes, contentType) => {
        const boundary = mime(contentType).parameters.get('boundary');
        if (!boundary) throw new TypeError('Multipart body has no boundary');
        const delimiter = '--' + boundary;
        const parts = binaryString(bytes).split(delimiter);
        // The multipart parser must fail rather than turn a missing or truncated
        // payload into an empty entry list. In particular, a null Fetch body with
        // a caller-supplied multipart Content-Type is not valid form data.
        if (parts.length < 2 || !parts.slice(1).some(part => part.startsWith('--')))
            throw new TypeError('Malformed multipart form data');
        const form = new FormData();
        for (let part of parts.slice(1)) {
            if (part.startsWith('--')) break;
            if (part.startsWith('\r\n')) part = part.slice(2);
            if (part.endsWith('\r\n')) part = part.slice(0, -2);
            const separator = part.indexOf('\r\n\r\n');
            if (separator < 0) continue;
            const headers = new Map();
            for (const line of part.slice(0, separator).split('\r\n')) {
                const split = line.indexOf(':');
                if (split > 0) headers.set(line.slice(0, split).trim().toLowerCase(), line.slice(split + 1).trim());
            }
            const disposition = headers.get('content-disposition') || '';
            const name = /(?:^|;)\s*name="([^"]*)"/i.exec(disposition)?.[1];
            if (name === undefined) continue;
            const filename = /(?:^|;)\s*filename="([^"]*)"/i.exec(disposition)?.[1];
            const bytes = binaryBytes(part.slice(separator + 4));
            if (filename === undefined) form.append(name, data.decoder.decode(bytes));
            else form.append(name, new File([bytes], filename, { type: headers.get('content-type') || 'text/plain' }));
        }
        return form;
    };
    const bodyMethods = {
        arrayBuffer() { return readBodyBytes(this).then(bytes => bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength)); },
        blob() { return readBodyBytes(this).then(bytes => new Blob([bytes], { type: this.headers?.get('content-type') || '' })); },
        bytes() { return readBodyBytes(this); },
        formData() {
            const contentType = this.headers?.get('content-type') || '';
            const parsed = mime(contentType);
            return readBodyBytes(this).then(bytes => {
                if (parsed.essence === 'application/x-www-form-urlencoded') {
                    const form = new FormData();
                    for (const [name, value] of new URLSearchParams(data.decoder.decode(bytes))) form.append(name, value);
                    return form;
                }
                if (parsed.essence === 'multipart/form-data') return multipartFormData(bytes, contentType);
                throw new TypeError('Body MIME type is not form data');
            });
        },
        json() { return this.text().then(JSON.parse); },
        text() { return readBodyBytes(this).then(bytes => data.decoder.decode(bytes)); }
    };
    const installBody = prototype => {
        Object.assign(prototype, bodyMethods);
        Object.defineProperties(prototype, {
            body: { configurable: true, enumerable: true, get() { return this.__bodyStream; } },
            bodyUsed: { configurable: true, enumerable: true, get() { return !!this.__bodyStream?.__disturbed; } }
        });
    };

    Object.defineProperty(globalThis, '__networkBody', {
        configurable: true,
        value: Object.freeze({
            initializeBody, bodyUnusable, readBodyBytes, cloneBody, transferBody, installBody
        })
    });
})();
