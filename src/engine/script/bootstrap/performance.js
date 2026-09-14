// Shared Window/Worker Performance Timeline. Continued by user_timing.js and performance_observer.js.
(() => {
    'use strict';
    const host = globalThis.__hostCall;
    const hooks = globalThis.__performanceHooks;
    delete globalThis.__performanceHooks;
    const serialize = globalThis.__serializeClone, deserialize = globalThis.__deserializeClone;
    const cloneDetail = value => value === undefined ? null : deserialize(serialize(value));
    const now = () => host('performanceNow');
    const timeOrigin = host('performanceTimeOrigin');
    const entries = [], entryState = new WeakMap(), performanceBrand = new WeakSet();
    const token = {};
    const domString = value => {
        if (typeof value === 'symbol') throw new TypeError('Cannot convert a Symbol to DOMString');
        return String(value);
    };
    const dictionary = value => {
        if (value == null) return {};
        if (typeof value !== 'object' && typeof value !== 'function') throw new TypeError('Expected dictionary');
        return value;
    };
    const finite = value => {
        const number = +value;
        if (!Number.isFinite(number)) throw new TypeError('Timestamp must be finite');
        return number;
    };
    const requireArgument = count => { if (!count) throw new TypeError('Missing required argument'); };
    const brand = (map, receiver) => {
        const state = map.get(receiver);
        if (!state) throw new TypeError('Illegal invocation');
        return state;
    };
    const checkPerformance = receiver => {
        if (receiver !== undefined && receiver !== globalThis && !performanceBrand.has(receiver))
            throw new TypeError('Illegal invocation');
    };
    const filterEntries = (buffer, name, type) => buffer.filter(entry => {
        const state = entryState.get(entry);
        return (name === undefined || state.name === name) && (type === undefined || state.entryType === type);
    }).sort((a,b) => entryState.get(a).startTime - entryState.get(b).startTime);
    const clearEntries = (type, name) => {
        for (let i = entries.length - 1; i >= 0; i--) {
            const state = entryState.get(entries[i]);
            if (state.entryType === type && (name === undefined || state.name === name)) entries.splice(i, 1);
        }
    };
    class PerformanceEntry {
        constructor(secret, state) {
            if (secret !== token) throw new TypeError('Illegal constructor');
            entryState.set(this, state);
        }
        get name() { return brand(entryState, this).name; }
        get entryType() { return brand(entryState, this).entryType; }
        get startTime() { return brand(entryState, this).startTime; }
        get duration() { return brand(entryState, this).duration; }
        toJSON() {
            const {name, entryType, startTime, duration} = brand(entryState, this);
            return {name, entryType, startTime, duration};
        }
    }
    const legacyNames = new Set(('navigationStart unloadEventStart unloadEventEnd redirectStart redirectEnd ' +
        'fetchStart domainLookupStart domainLookupEnd connectStart connectEnd secureConnectionStart ' +
        'requestStart responseStart responseEnd domLoading domInteractive domContentLoadedEventStart ' +
        'domContentLoadedEventEnd domComplete loadEventStart loadEventEnd').split(' '));
    const isWindow = typeof globalThis.document === 'object';
    // No invented Navigation Timing entries. Only the realm origin is currently available.
    const timingValues = Object.fromEntries([...legacyNames].map(name =>
        [name, name === 'navigationStart' ? timeOrigin : 0]));
    const legacyTiming = Object.freeze({...timingValues, toJSON() { return {...timingValues}; }});
    const timestamp = value => {
        if (typeof value !== 'string') {
            const result = finite(value);
            if (result < 0) throw new TypeError('Timestamp must be non-negative');
            return result;
        }
        if (isWindow && legacyNames.has(value)) {
            if (!legacyTiming[value]) throw new DOMException('Timing attribute is unavailable', 'InvalidAccessError');
            return legacyTiming[value] - timeOrigin;
        }
        for (let i = entries.length - 1; i >= 0; i--) {
            const state = entryState.get(entries[i]);
            if (state.entryType === 'mark' && state.name === value) return state.startTime;
        }
        throw new DOMException(`The mark '${value}' does not exist`, 'SyntaxError');
    };
