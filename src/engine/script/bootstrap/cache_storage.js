// Cache API requests share the bounded storage IPC lane, but use a separate
// browser-owned, origin-partitioned store. See Service Workers §5 (Caches).
(() => {
    'use strict';
    const host = (...args) => __hostCall(...args);
    if (!host('cacheStorageSecureContext')) return;

    const pending = new Map();
    const cacheNames = new WeakMap();
    const cacheStorageBrand = new WeakSet();
    const token = {};
    const MAX_BODY_BYTES = 2 * 1024 * 1024;
    // Retain the bootstrap Fetch implementations, not mutable window globals.
    const NativeRequest = Request;
    const NativeResponse = Response;
    const nativeFetch = fetch;
    const responseFromNetwork = NativeResponse.__fromNetwork.bind(NativeResponse);
    const cloneResponse = NativeResponse.prototype.clone;
    const readResponseBytes = NativeResponse.prototype.bytes;
    const assertCache = value => {
        if (!cacheNames.has(value)) throw new TypeError('Illegal Cache receiver');
    };
    const assertStorage = value => {
        if (!cacheStorageBrand.has(value)) throw new TypeError('Illegal CacheStorage receiver');
    };

    const send = command => new Promise((resolve, reject) => {
        try {
            const id = Number(host('databaseRequest', JSON.stringify({kind:'cache', command})));
            pending.set(id, {resolve, reject});
        } catch (error) { reject(error); }
    });
    globalThis.__receiveCacheStorageEvent = (id, payload) => {
        const operation = pending.get(Number(id));
        if (!operation) return;
        pending.delete(Number(id));
        let reply;
        try { reply = JSON.parse(String(payload)); }
        catch (_) { return operation.reject(new DOMException('Malformed CacheStorage response', 'UnknownError')); }
        if (reply.kind === 'error') {
            const name = String(reply.name || 'UnknownError');
            return operation.reject(name === 'TypeError' ? new TypeError(String(reply.message || 'Cache API error'))
                : new DOMException(String(reply.message || 'Cache API error'), name));
        }
        if (reply.kind !== 'cache')
            return operation.reject(new DOMException('Malformed CacheStorage response', 'UnknownError'));
        operation.resolve(reply.value);
    };

    const required = (args, name) => {
        if (args.length === 0) throw new TypeError(`${name} requires a request`);
    };
    const domString = value => {
        if (typeof value === 'symbol') throw new TypeError('Symbol is not a DOMString');
        return String(value);
    };
    const cacheName = (value, args, name) => {
        required(args, name);
        return domString(value);
    };
    const requestFrom = input => input instanceof NativeRequest ? input : new NativeRequest(input);
    const requestRecord = input => {
        const request = requestFrom(input);
        return {url: request.url, method: request.method, headers: [...request.headers]};
    };
    const queryOptions = options => ({
        ignoreSearch: !!options?.ignoreSearch,
        ignoreMethod: !!options?.ignoreMethod,
        ignoreVary: !!options?.ignoreVary
    });
    const requestList = records => records.map(record => new NativeRequest(record.url,
        {method: record.method, headers: record.headers}));

    const bytesToBase64 = bytes => {
        let binary = '';
        for (let start = 0; start < bytes.length; start += 0x4000)
            binary += String.fromCharCode(...bytes.subarray(start, start + 0x4000));
        return btoa(binary);
    };
    const base64ToBytes = source => {
        const binary = atob(source);
        const bytes = new Uint8Array(binary.length);
        for (let index = 0; index < binary.length; index++) bytes[index] = binary.charCodeAt(index);
        return bytes;
    };
    const responseFrom = record => {
        if (record === null) return undefined;
        const bytes = record.bodyBase64 === null ? null : base64ToBytes(record.bodyBase64);
        return responseFromNetwork({
            status: record.status, statusText: record.statusText,
            responseType: record.responseType, url: record.url,
            redirected: record.redirected, headers: record.headers
        }, bytes, bytes === null);
    };
    const validatePut = (request, response) => {
        if (!(response instanceof NativeResponse)) throw new TypeError('Cache.put requires a Response');
        if (request.method !== 'GET' || !/^https?:$/.test(new URL(request.url).protocol))
            throw new TypeError('Cache.put requires an HTTP(S) GET request');
        if (response.status === 206 || response.type === 'error' ||
            !['default', 'basic', 'cors'].includes(response.type))
            throw new TypeError('The Response cannot be stored in this Cache');
        if ((response.headers.get('vary') || '').split(',').some(field => field.trim() === '*'))
            throw new TypeError('Vary: * responses cannot be stored in a Cache');
        if (response.bodyUsed || response.body?.locked)
            throw new TypeError('The Response body is already used');
    };
    const entryFrom = (input, response) => {
        const request = requestFrom(input);
        validatePut(request, response);
        const hadBody = response.body !== null;
        const copy = cloneResponse.call(response);
        const metadata = {
            status: response.status, statusText: response.statusText,
            responseType: response.type, url: response.url,
            redirected: response.redirected, headers: [...response.headers]
        };
        const storedRequest = requestRecord(request);
        return readResponseBytes.call(copy).then(bytes => {
            if (bytes.length > MAX_BODY_BYTES) throw new DOMException('Cache response exceeds the browser limit', 'QuotaExceededError');
            return {
                request: storedRequest,
                response: {...metadata, bodyBase64: hadBody ? bytesToBase64(bytes) : null}
            };
        });
    };

    class Cache {
        constructor(key, name) {
            if (key !== token) throw new TypeError('Illegal constructor');
            cacheNames.set(this, name);
        }
        match(request, options = {}) {
            assertCache(this);
            required(arguments, 'Cache.match');
            return Promise.resolve().then(() => requestRecord(request))
                .then(record => send({op:'match', name:cacheNames.get(this), request:record,
                    options:queryOptions(options)})).then(responseFrom);
        }
        matchAll(request = undefined, options = {}) {
            assertCache(this);
            return Promise.resolve().then(() => request === undefined ? null : requestRecord(request))
                .then(record => send({op:'matchAll', name:cacheNames.get(this), request:record,
                    options:queryOptions(options)})).then(records => records.map(responseFrom));
        }
        keys(request = undefined, options = {}) {
            assertCache(this);
            return Promise.resolve().then(() => request === undefined ? null : requestRecord(request))
                .then(record => send({op:'keys', name:cacheNames.get(this), request:record,
                    options:queryOptions(options)})).then(requestList);
        }
        put(request, response) {
            assertCache(this);
            if (arguments.length < 2) throw new TypeError('Cache.put requires a request and Response');
            return Promise.resolve().then(() => entryFrom(request, response))
                .then(entry => send({op:'put', name:cacheNames.get(this), entries:[entry]}));
        }
        add(request) {
            assertCache(this);
            required(arguments, 'Cache.add');
            return Cache.prototype.addAll.call(this, [request]);
        }
        addAll(requests) {
            assertCache(this);
            if (arguments.length === 0) throw new TypeError('Cache.addAll requires requests');
            return Promise.resolve().then(() => {
                if (requests == null || typeof requests[Symbol.iterator] !== 'function')
                    throw new TypeError('Cache.addAll requires an iterable of requests');
                return Array.from(requests, requestFrom);
            })
                .then(requests => Promise.all(requests.map(request => {
                    if (request.method !== 'GET') throw new TypeError('Cache.addAll requires GET requests');
                    return nativeFetch(request).then(response => {
                        if (!response.ok) throw new TypeError('Cache.addAll fetch was not successful');
                        return entryFrom(request, response);
                    });
                }))).then(entries => entries.length ? send({op:'put', name:cacheNames.get(this), entries}) : undefined);
        }
        delete(request, options = {}) {
            assertCache(this);
            required(arguments, 'Cache.delete');
            return Promise.resolve().then(() => requestRecord(request))
                .then(record => send({op:'deleteEntry', name:cacheNames.get(this),
                    request:record, options:queryOptions(options)}));
        }
    }

    class CacheStorage {
        constructor(key) {
            if (key !== token) throw new TypeError('Illegal constructor');
            cacheStorageBrand.add(this);
        }
        open(name) {
            assertStorage(this);
            name = cacheName(name, arguments, 'CacheStorage.open');
            return send({op:'open', name}).then(() => new Cache(token, name));
        }
        has(name) {
            assertStorage(this);
            name = cacheName(name, arguments, 'CacheStorage.has');
            return send({op:'has', name});
        }
        keys() {
            assertStorage(this);
            return send({op:'names'});
        }
        delete(name) {
            assertStorage(this);
            name = cacheName(name, arguments, 'CacheStorage.delete');
            return send({op:'deleteCache', name});
        }
        match(request, options = {}) {
            assertStorage(this);
            required(arguments, 'CacheStorage.match');
            const name = options?.cacheName === undefined ? null : domString(options.cacheName);
            return Promise.resolve().then(() => requestRecord(request))
                .then(record => send({op:'match', name, request:record, options:queryOptions(options)}))
                .then(responseFrom);
        }
    }
    Object.defineProperty(Cache.prototype, Symbol.toStringTag, {value:'Cache', configurable:true});
    Object.defineProperty(CacheStorage.prototype, Symbol.toStringTag,
        {value:'CacheStorage', configurable:true});
    Object.assign(globalThis, {Cache, CacheStorage});
    Object.defineProperty(globalThis, 'caches', {
        configurable: true, enumerable: true, value: new CacheStorage(token)
    });
})();
