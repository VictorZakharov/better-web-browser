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
    const decoderInputView = input => {
        if (input === undefined) return new Uint8Array();
        if (input instanceof ArrayBuffer) return new Uint8Array(input);
        if (ArrayBuffer.isView?.(input)) return new Uint8Array(input.buffer, input.byteOffset, input.byteLength);
        throw new TypeError('input must be an ArrayBuffer or an ArrayBuffer view');
    };
    class TextDecoder {
        constructor(label = 'utf-8', options = {}) {
            label = String(label).trim().toLowerCase();
            if (!['utf-8', 'utf8', 'unicode-1-1-utf-8'].includes(label))
                throw new RangeError('Only UTF-8 decoding is implemented');
            this.__fatal = !!options.fatal;
            this.__ignoreBOM = !!options.ignoreBOM;
            this.__pending = new Uint8Array();
            this.__streaming = false;
            this.__bomSeen = false;
        }
        get encoding() { return 'utf-8'; }
        get fatal() { return this.__fatal; }
        get ignoreBOM() { return this.__ignoreBOM; }
        decode(input, options = {}) {
            const stream = !!options.stream;
            const result = host('utf8Decode', decoderInputView(input),
                this.__streaming ? this.__pending : new Uint8Array(),
                stream, this.__fatal, this.__ignoreBOM, this.__bomSeen);
            this.__pending = result[1];
            this.__streaming = stream;
            this.__bomSeen = result[2];
            if (!stream) {
                this.__pending = new Uint8Array();
                this.__bomSeen = false;
            }
            return result[0];
        }
    }
    Object.assign(globalThis, { TextEncoder, TextDecoder });

    const timers = new Map(); let nextTimer = 1;
    const queueTimer = (callback, delay, repeat, args, operation = 'timerSchedule') => {
        const id = nextTimer++; timers.set(id, { callback, repeat, args, cancelable: operation === 'timerSchedule' });
        host(operation, id, Math.max(0, Number(delay) || 0), repeat); return id;
    };
    globalThis.setTimeout = (callback, delay, ...args) => queueTimer(callback, delay, false, args);
    globalThis.setInterval = (callback, delay, ...args) => queueTimer(callback, delay, true, args);
    globalThis.clearTimeout = globalThis.clearInterval = id => {
        id = Number(id); if (timers.get(id)?.cancelable === false) return;
        timers.delete(id); host('timerCancel', id);
    };
    globalThis.__runTimer = id => {
        const timer = timers.get(Number(id)); if (!timer) return;
        if (!timer.repeat) timers.delete(Number(id));
        if (typeof timer.callback === 'function') timer.callback(...timer.args); else (0, eval)(String(timer.callback));
    };
    let reportingGlobalException = false;
    const reportGlobalException = (error, source) => {
        const message = error?.message === undefined ? String(error) : String(error.message);
        const event = markTrusted(new ErrorEvent('error', {cancelable: true, message, error}));
        let uncanceled = true;
        if (!reportingGlobalException) {
            reportingGlobalException = true;
            try { uncanceled = globalThis.dispatchEvent(event); }
            finally { reportingGlobalException = false; }
        }
        if (uncanceled) host('console', 'error', 'Uncaught ' + source + ' exception: ' + message);
    };
    globalThis.__performanceHooks = {
        queue: callback => queueTimer(callback, 0, false, [], 'performanceTaskSchedule'),
        report: error => reportGlobalException(error, 'PerformanceObserver')
    };
    globalThis.queueMicrotask = callback => {
        if (typeof callback !== 'function') throw new TypeError('queueMicrotask requires a callback');
        host('queueMicrotask', () => {
            try { callback(); }
            catch (error) { reportGlobalException(error, 'microtask'); }
        });
    };
    globalThis.navigator = { userAgent: host('userAgent'), language: 'en-CA', languages: ['en-CA', 'en'], onLine: true, hardwareConcurrency: 1 };
    globalThis.location = new URL(host('workerLocation'));
    globalThis.name = host('workerName');
    globalThis.console = Object.fromEntries(['log', 'info', 'warn', 'error', 'debug'].map(level => [level,
        (...args) => host('console', level, args.map(value => String(value)).join(' '))]));
    globalThis.structuredClone = value => JSON.parse(JSON.stringify(value));
})();
