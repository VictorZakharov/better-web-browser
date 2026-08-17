(() => {
    'use strict';
    const host = (...args) => __hostCall(...args);
    globalThis.self = globalThis;
    const trustedEvents = new WeakSet();
    const markTrusted = event => { trustedEvents.add(event); return event; };
    Object.defineProperty(globalThis, '__markTrustedEvent', {
        value: markTrusted, configurable: true,
    });

    class Event {
        constructor(type, init = {}) {
            this.type = String(type); this.bubbles = !!init.bubbles; this.cancelable = !!init.cancelable;
            this.target = null; this.currentTarget = null; this.defaultPrevented = false;
            this.timeStamp = Date.now(); this.__stopped = false;
        }
        get isTrusted() { return trustedEvents.has(this); }
        preventDefault() { if (this.cancelable) this.defaultPrevented = true; }
        stopPropagation() { this.__stopped = true; }
        stopImmediatePropagation() { this.__stopped = true; this.__immediate = true; }
    }
    class MessageEvent extends Event {
        constructor(type, init = {}) {
            super(type, init); this.data = init.data ?? null; this.origin = String(init.origin || '');
            this.lastEventId = String(init.lastEventId || ''); this.source = init.source ?? null;
            this.ports = Object.freeze([...(init.ports || [])]);
        }
    }
    class ErrorEvent extends Event {
        constructor(type, init = {}) {
            super(type, init); this.message = String(init.message || ''); this.filename = String(init.filename || '');
            this.lineno = Number(init.lineno) || 0; this.colno = Number(init.colno) || 0; this.error = init.error ?? null;
        }
    }
    const listeners = new WeakMap();
    class EventTarget {
        addEventListener(type, callback, options = {}) {
            if (callback == null) return;
            const list = listeners.get(this) || [];
            if (!list.some(item => item.type === String(type) && item.callback === callback))
                list.push({ type: String(type), callback, once: !!options?.once });
            listeners.set(this, list);
        }
        removeEventListener(type, callback) {
            const list = listeners.get(this);
            if (list) listeners.set(this, list.filter(item => item.type !== String(type) || item.callback !== callback));
        }
        dispatchEvent(event) {
            if (!(event instanceof Event)) throw new TypeError('dispatchEvent requires an Event');
            event.target = event.currentTarget = this;
            for (const item of [...(listeners.get(this) || [])]) {
                if (item.type !== event.type) continue;
                if (item.once) this.removeEventListener(item.type, item.callback);
                try {
                    if (typeof item.callback === 'function') item.callback.call(this, event);
                    else item.callback?.handleEvent?.call(item.callback, event);
                } catch (error) { host('console', 'error', error?.stack || String(error)); }
                if (event.__immediate) break;
            }
            event.currentTarget = null;
            return !event.defaultPrevented;
        }
    }
    Object.assign(globalThis, { Event, MessageEvent, ErrorEvent, EventTarget });

    const formDecode = value => decodeURIComponent(String(value).replace(/\+/g, ' '));
    const formEncode = value => encodeURIComponent(String(value)).replace(/%20/g, '+');
    class URLSearchParams {
        constructor(init = '') {
            this.__entries = [];
            if (typeof init === 'string') {
                const source = init.replace(/^\?/, '');
                if (source) for (const part of source.split('&')) {
                    const split = part.indexOf('=');
                    this.append(formDecode(split < 0 ? part : part.slice(0, split)), formDecode(split < 0 ? '' : part.slice(split + 1)));
                }
            } else if (init != null && typeof init[Symbol.iterator] === 'function') {
                for (const pair of init) this.append(pair[0], pair[1]);
            } else if (init != null) for (const key of Object.keys(Object(init))) this.append(key, init[key]);
        }
        append(name, value) { this.__entries.push([String(name), String(value)]); }
        delete(name) { name = String(name); this.__entries = this.__entries.filter(entry => entry[0] !== name); }
        get(name) { return this.__entries.find(entry => entry[0] === String(name))?.[1] ?? null; }
        getAll(name) { return this.__entries.filter(entry => entry[0] === String(name)).map(entry => entry[1]); }
        has(name) { return this.__entries.some(entry => entry[0] === String(name)); }
        set(name, value) { this.delete(name); this.append(name, value); }
        sort() { this.__entries.sort((a, b) => a[0].localeCompare(b[0])); }
        forEach(callback, thisArg) { for (const [name, value] of this.__entries) callback.call(thisArg, value, name, this); }
        entries() { return this.__entries.map(entry => [...entry])[Symbol.iterator](); }
        keys() { return this.__entries.map(entry => entry[0])[Symbol.iterator](); }
        values() { return this.__entries.map(entry => entry[1])[Symbol.iterator](); }
        [Symbol.iterator]() { return this.entries(); }
        toString() { return this.__entries.map(([name, value]) => formEncode(name) + '=' + formEncode(value)).join('&'); }
    }
    const missingUrlValue = {};
    class URL {
        constructor(value = missingUrlValue, base = host('workerLocation')) {
            if (value === missingUrlValue) throw new TypeError('URL requires an input');
            this.href = host('strictResolveUrl', String(value), String(base));
        }
        static canParse(value, base = host('workerLocation')) {
            try { host('strictResolveUrl', String(value), String(base)); return true; }
            catch (_) { return false; }
        }
        static parse(value, base = host('workerLocation')) {
            try { return new URL(value, base); } catch (_) { return null; }
        }
        toString() { return this.href; }
        toJSON() { return this.href; }
        get protocol() { return this.href.match(/^([^:]+:)/)?.[1] || ''; }
        get origin() { return this.href.match(/^([^:]+:\/\/[^/]+)/)?.[1] || 'null'; }
        get host() { return this.href.match(/^[^:]+:\/\/([^/]+)/)?.[1] || ''; }
        get hostname() { return this.host.replace(/:\d+$/, ''); }
        get pathname() { return this.href.match(/^[^:]+:\/\/[^/]+([^?#]*)/)?.[1] || '/'; }
        get search() { return this.href.match(/(\?[^#]*)/)?.[1] || ''; }
        get hash() { return this.href.match(/(#.*)$/)?.[1] || ''; }
        get searchParams() { return new URLSearchParams(this.search); }
    }
    Object.assign(globalThis, { URL, URLSearchParams });

    const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
    globalThis.btoa = value => {
        const input = String(value); let output = '', buffer = 0, bits = 0;
        for (let index = 0; index < input.length; index++) {
            const code = input.charCodeAt(index); if (code > 255) throw new DOMException('Invalid character', 'InvalidCharacterError');
            buffer = (buffer << 8) | code; bits += 8;
            while (bits >= 6) { bits -= 6; output += alphabet[(buffer >> bits) & 63]; }
        }
        if (bits) output += alphabet[(buffer << (6 - bits)) & 63];
        while (output.length % 4) output += '=';
        return output;
    };
    globalThis.atob = value => {
        const input = String(value).replace(/[\t\n\f\r ]/g, '').replace(/=+$/, '');
        if (input.length % 4 === 1 || /[^A-Za-z0-9+/]/.test(input)) throw new DOMException('Invalid data', 'InvalidCharacterError');
        let output = '', buffer = 0, bits = 0;
        for (const character of input) {
            buffer = (buffer << 6) | alphabet.indexOf(character); bits += 6;
            if (bits >= 8) { bits -= 8; output += String.fromCharCode((buffer >> bits) & 255); }
        }
        return output;
    };

    class TextEncoder {
        get encoding() { return 'utf-8'; }
        encode(value = '') {
            const encoded = unescape(encodeURIComponent(String(value)));
            return new Uint8Array([...encoded].map(character => character.charCodeAt(0)));
        }
        encodeInto(value, destination) {
            const bytes = this.encode(value); const written = Math.min(bytes.length, destination.length);
            destination.set(bytes.subarray(0, written)); return { read: String(value).length, written };
        }
    }
    class TextDecoder {
        constructor(label = 'utf-8', options = {}) {
            if (!['utf-8', 'utf8'].includes(String(label).toLowerCase())) throw new RangeError('Only UTF-8 is supported');
            this.fatal = !!options.fatal; this.ignoreBOM = !!options.ignoreBOM; this.encoding = 'utf-8';
        }
        decode(input = new Uint8Array()) {
            const bytes = input instanceof ArrayBuffer ? new Uint8Array(input) : new Uint8Array(input.buffer, input.byteOffset, input.byteLength);
            let encoded = ''; for (const byte of bytes) encoded += '%' + byte.toString(16).padStart(2, '0');
            try { return decodeURIComponent(encoded); }
            catch (_) { if (this.fatal) throw new TypeError('Invalid UTF-8'); return '\ufffd'; }
        }
    }
    Object.assign(globalThis, { TextEncoder, TextDecoder });

    const timers = new Map(); let nextTimer = 1;
    const queueTimer = (callback, delay, repeat, args) => {
        const id = nextTimer++; timers.set(id, { callback, repeat, args });
        host('timerSchedule', id, Math.max(0, Number(delay) || 0), repeat); return id;
    };
    globalThis.setTimeout = (callback, delay, ...args) => queueTimer(callback, delay, false, args);
    globalThis.setInterval = (callback, delay, ...args) => queueTimer(callback, delay, true, args);
    globalThis.clearTimeout = globalThis.clearInterval = id => { timers.delete(Number(id)); host('timerCancel', Number(id)); };
    globalThis.__runTimer = id => {
        const timer = timers.get(Number(id)); if (!timer) return;
        if (!timer.repeat) timers.delete(Number(id));
        if (typeof timer.callback === 'function') timer.callback(...timer.args); else (0, eval)(String(timer.callback));
    };
    const started = Date.now();
    globalThis.performance = { timeOrigin: started, now: () => Date.now() - started };
    globalThis.queueMicrotask = callback => Promise.resolve().then(callback);
    globalThis.navigator = { userAgent: host('userAgent'), language: 'en-CA', languages: ['en-CA', 'en'], onLine: true, hardwareConcurrency: 1 };
    globalThis.location = new URL(host('workerLocation'));
    globalThis.name = host('workerName');
    globalThis.console = Object.fromEntries(['log', 'info', 'warn', 'error', 'debug'].map(level => [level,
        (...args) => host('console', level, args.map(value => String(value)).join(' '))]));
    globalThis.structuredClone = value => JSON.parse(JSON.stringify(value));
})();
