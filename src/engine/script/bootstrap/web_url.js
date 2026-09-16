(() => {
    'use strict';
    const host = (...args) => __hostCall(...args);
    const urls = new WeakMap(), params = new WeakMap();
    const usv = value => {
        if (typeof value === 'symbol') throw new TypeError('Cannot convert a Symbol to a string');
        return String(value).toWellFormed();
    };
    const required = (count, minimum) => {
        if (count < minimum) throw new TypeError('Not enough arguments');
    };
    const state = (map, receiver) => {
        const value = map.get(receiver);
        if (!value) throw new TypeError('Illegal invocation');
        return value;
    };
    const parsePairs = value => JSON.parse(host('parseFormUrlencoded', value.replace(/^\?/, '')));
    const encode = value => encodeURIComponent(value)
        .replace(/[!'()~]/g, character => '%' + character.charCodeAt(0).toString(16).toUpperCase())
        .replace(/%20/g, '+');
    const serialize = value => value.list.map(([name, item]) => encode(name) + '=' + encode(item)).join('&');
    const changed = value => {
        if (value.url) value.url.href = host('setWebUrlComponent', value.url.href, 'search', serialize(value));
    };
    // Iterators consult the current list, including replacements by URL.search/href.
    // https://url.spec.whatwg.org/#urlsearchparams-iteration-behavior
    const iterate = (receiver, kind) => {
        const value = state(params, receiver);
        let index = 0, done = false;
        return {
            next() {
                if (done || index >= value.list.length) { done = true; return { value: undefined, done: true }; }
                const pair = value.list[index++];
                return { value: kind === 'keys' ? pair[0] : kind === 'values' ? pair[1] : [...pair], done: false };
            },
            [Symbol.iterator]() { return this; },
            [Symbol.toStringTag]: 'URLSearchParams Iterator'
        };
    };
    class URLSearchParams {
        constructor(init = '') {
            if (init === null) init = '';
            let list = [];
            if (init !== null && (typeof init === 'object' || typeof init === 'function')) {
                const iterator = init[Symbol.iterator];
                if (iterator != null) {
                    if (typeof iterator !== 'function') throw new TypeError('Invalid iterator');
                    for (const pair of { [Symbol.iterator]: () => iterator.call(init) }) {
                        if (pair === null || (typeof pair !== 'object' && typeof pair !== 'function'))
                            throw new TypeError('Expected a sequence of pairs');
                        const values = [];
                        for (const value of pair) values.push(usv(value));
                        if (values.length !== 2) throw new TypeError('Expected a pair of strings');
                        list.push(values);
                    }
                } else {
                    const record = new Map();
                    for (const key of Reflect.ownKeys(init)) {
                        if (Object.getOwnPropertyDescriptor(init, key)?.enumerable) {
                            const name = usv(key);
                            record.set(name, usv(init[key]));
                        }
                    }
                    list = [...record];
                }
            } else list = parsePairs(usv(init));
            params.set(this, { list, url: null });
        }
        get size() { return state(params, this).list.length; }
        append(name, value) {
            const data = state(params, this); required(arguments.length, 2);
            data.list.push([usv(name), usv(value)]); changed(data);
        }
        delete(name, value = undefined) {
            const data = state(params, this); required(arguments.length, 1);
            name = usv(name); value = value === undefined ? undefined : usv(value);
            data.list = data.list.filter(pair => pair[0] !== name || value !== undefined && pair[1] !== value);
            changed(data);
        }
        get(name) {
            const data = state(params, this); required(arguments.length, 1); name = usv(name);
            return data.list.find(pair => pair[0] === name)?.[1] ?? null;
        }
        getAll(name) {
            const data = state(params, this); required(arguments.length, 1); name = usv(name);
            return data.list.filter(pair => pair[0] === name).map(pair => pair[1]);
        }
        has(name, value = undefined) {
            const data = state(params, this); required(arguments.length, 1);
            name = usv(name); value = value === undefined ? undefined : usv(value);
            return data.list.some(pair => pair[0] === name && (value === undefined || pair[1] === value));
        }
        set(name, value) {
            const data = state(params, this); required(arguments.length, 2);
            name = usv(name); value = usv(value);
            let replaced = false;
            data.list = data.list.filter(pair => {
                if (pair[0] !== name) return true;
                if (replaced) return false;
                pair[1] = value; replaced = true; return true;
            });
            if (!replaced) data.list.push([name, value]);
            changed(data);
        }
        sort() {
            const data = state(params, this);
            data.list.sort((left, right) => left[0] < right[0] ? -1 : left[0] > right[0] ? 1 : 0);
            changed(data);
        }
        forEach(callback, thisArg = undefined) {
            const data = state(params, this);
            if (typeof callback !== 'function') throw new TypeError('Expected a callback');
            for (let index = 0; index < data.list.length; index++) {
                const [name, value] = data.list[index]; callback.call(thisArg, value, name, this);
            }
        }
        entries() { return iterate(this, 'entries'); }
        keys() { return iterate(this, 'keys'); }
        values() { return iterate(this, 'values'); }
        toString() { return serialize(state(params, this)); }
    }
    Object.defineProperty(URLSearchParams.prototype, Symbol.iterator, {
        value: URLSearchParams.prototype.entries, configurable: true, writable: true
    });
    const parts = href => JSON.parse(host('parseWebUrl', href));
    const initialize = (receiver, href) => {
        const searchParams = new URLSearchParams(parts(href).search);
        const value = { href, searchParams };
        urls.set(receiver, value); params.get(searchParams).url = value;
        return receiver;
    };
    const parsedInput = (value, base, count) => {
        required(count, 1);
        // Web IDL conversion exceptions must not become null/false in static APIs.
        value = usv(value); base = base === undefined ? undefined : usv(base);
        try { return host('strictResolveUrl', value, base); } catch (_) { return null; }
    };
    const getPart = (receiver, name) => parts(state(urls, receiver).href)[name];
    const setPart = (receiver, name, value) => {
        const data = state(urls, receiver); value = usv(value);
        try { data.href = host('setWebUrlComponent', data.href, name, value); }
        catch (error) { if (name === 'href') throw error; return; }
        if (name === 'href' || name === 'search')
            params.get(data.searchParams).list = parsePairs(parts(data.href).search);
    };
    class URL {
        constructor(value, base = undefined) {
            const href = parsedInput(value, base, arguments.length);
            if (href === null) throw new TypeError('Invalid URL');
            initialize(this, href);
        }
        static canParse(value, base = undefined) { return parsedInput(value, base, arguments.length) !== null; }
        static parse(value, base = undefined) {
            const href = parsedInput(value, base, arguments.length);
            return href === null ? null : initialize(Object.create(URL.prototype), href);
        }
        toString() { return state(urls, this).href; }
        toJSON() { return state(urls, this).href; }
        get href() { return state(urls, this).href; }
        set href(value) { setPart(this, 'href', value); }
        get origin() { return getPart(this, 'origin'); }
        get protocol() { return getPart(this, 'protocol'); }
        set protocol(value) { setPart(this, 'protocol', value); }
        get username() { return getPart(this, 'username'); }
        set username(value) { setPart(this, 'username', value); }
        get password() { return getPart(this, 'password'); }
        set password(value) { setPart(this, 'password', value); }
        get host() { return getPart(this, 'host'); }
        set host(value) { setPart(this, 'host', value); }
        get hostname() { return getPart(this, 'hostname'); }
        set hostname(value) { setPart(this, 'hostname', value); }
        get port() { return getPart(this, 'port'); }
        set port(value) { setPart(this, 'port', value); }
        get pathname() { return getPart(this, 'pathname'); }
        set pathname(value) { setPart(this, 'pathname', value); }
        get search() { return getPart(this, 'search'); }
        set search(value) { setPart(this, 'search', value); }
        get hash() { return getPart(this, 'hash'); }
        set hash(value) { setPart(this, 'hash', value); }
        get searchParams() { return state(urls, this).searchParams; }
    }
    for (const [type, name] of [[URL, 'URL'], [URLSearchParams, 'URLSearchParams']]) {
        Object.defineProperty(type.prototype, Symbol.toStringTag, { value: name, configurable: true });
        for (const key of Object.getOwnPropertyNames(type.prototype)) {
            if (key !== 'constructor') Object.defineProperty(type.prototype, key, { enumerable: true });
        }
    }
    Object.assign(globalThis, { URL, URLSearchParams });
    // Bootstrap-only handoff. Consumers capture native algorithms, not public
    // constructors or prototype getters; the handoff is deleted before author JS.
    globalThis.__urlInternals = {
        usv, parts,
        resolve: value => host('strictResolveUrl', usv(value), host('apiBaseUrl')),
        origin: () => parts(host('apiOriginUrl')).origin,
        isParams: value => params.has(value),
        serializeParams: value => serialize(state(params, value)),
        parseFormBytes: bytes => JSON.parse(host('parseFormUrlencodedBytes', bytes))
    };
})();
