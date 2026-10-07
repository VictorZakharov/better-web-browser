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
        preventDefault() { if (this.cancelable && !this.__passive) this.defaultPrevented = true; }
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
    let isNativeAbortSignal = () => false;
    Object.defineProperty(globalThis, '__installAbortSignalBrand', {
        configurable: true, value: checker => { isNativeAbortSignal = checker; }
    });
    const recordsFor = target => {
        let records = listeners.get(target);
        if (!records) listeners.set(target, records = []);
        return records;
    };
    const captureOption = options => typeof options === 'boolean' ? options : !!options?.capture;
    const removeRecord = (target, record) => {
        if (record.removed) return;
        record.removed = true;
        const records = listeners.get(target);
        const index = records?.indexOf(record) ?? -1;
        if (index >= 0) records.splice(index, 1);
        if (record.abortRecord) removeRecord(record.signal, record.abortRecord);
    };
    const addRecord = (target, type, callback, options) => {
        type = String(type);
        const capture = captureOption(options);
        const dictionary = typeof options === 'boolean' || options == null ? null : Object(options);
        const once = !!dictionary?.once;
        const passive = !!dictionary?.passive;
        const signal = dictionary?.signal;
        if (signal != null && !isNativeAbortSignal(signal))
            throw new TypeError('Event listener signal must be an AbortSignal');
        if (signal?.aborted || callback == null) return;
        if (typeof callback !== 'function' && typeof callback !== 'object') return;
        const records = recordsFor(target);
        if (records.some(record => !record.removed && record.type === type &&
            record.callback === callback && record.capture === capture)) return;
        const record = { type, callback, capture, once, passive, signal, removed: false };
        records.push(record);
        if (signal) record.abortRecord = addRecord(signal, 'abort',
            () => removeRecord(target, record), { once: true });
        return record;
    };
    class EventTarget {
        addEventListener(type, callback, options = {}) {
            addRecord(this, type, callback, options);
        }
        removeEventListener(type, callback, options = {}) {
            const capture = captureOption(options);
            const record = listeners.get(this)?.find(candidate => !candidate.removed &&
                candidate.type === String(type) && candidate.callback === callback &&
                candidate.capture === capture);
            if (record) removeRecord(this, record);
        }
        dispatchEvent(event) {
            if (!(event instanceof Event)) throw new TypeError('dispatchEvent requires an Event');
            event.target = event.currentTarget = this;
            for (const item of [...(listeners.get(this) || [])]) {
                if (item.removed || item.type !== event.type) continue;
                if (item.once) removeRecord(this, item);
                event.__passive = item.passive;
                try {
                    if (typeof item.callback === 'function') item.callback.call(this, event);
                    else item.callback?.handleEvent?.call(item.callback, event);
                } catch (error) { host('console', 'error', error?.stack || String(error)); }
                finally { event.__passive = false; }
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

    const timers = new Map(); let nextTimer = 1;
    const queueTimer = (callback, delay, repeat, args, operation = 'timerSchedule') => {
        const id = nextTimer++; timers.set(id, { callback, repeat, args, cancelable: operation === 'timerSchedule' });
        host(operation, id, Math.max(0, Number(delay) || 0), repeat); return id;
    };
    globalThis.setTimeout = (callback, delay, ...args) => queueTimer(callback, delay, false, args);
    globalThis.__webGlContextTask = callback => queueTimer(callback, 0, false, [], 'mediaCapabilitiesTaskSchedule');
    globalThis.__fontLoadingQueue = callback => queueTimer(callback, 0, false, [], 'fontTaskSchedule');
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
    globalThis.__mediaCapabilitiesQueue = callback =>
        queueTimer(callback, 0, false, [], 'mediaCapabilitiesTaskSchedule');
    globalThis.queueMicrotask = callback => {
        if (typeof callback !== 'function') throw new TypeError('queueMicrotask requires a callback');
        host('queueMicrotask', () => {
            try { callback(); }
            catch (error) { reportGlobalException(error, 'microtask'); }
        });
    };
    globalThis.navigator = { userAgent: host('userAgent'), language: 'en-CA', languages: ['en-CA', 'en'], onLine: true };
    const hardwareConcurrency = host('hardwareConcurrency');
    Object.defineProperty(globalThis.navigator, 'hardwareConcurrency', {
        get() { return hardwareConcurrency; }, enumerable: true, configurable: true
    });
    globalThis.location = new URL(host('workerLocation'));
    globalThis.name = host('workerName');
    globalThis.console = Object.fromEntries(['log', 'info', 'warn', 'error', 'debug'].map(level => [level,
        (...args) => host('console', level, args.map(value => String(value)).join(' '))]));
    globalThis.structuredClone = value => JSON.parse(JSON.stringify(value));
})();
