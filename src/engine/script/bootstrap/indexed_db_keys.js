(() => {
    'use strict';
    // IndexedDB keys have a type order and reject NaN, unlike JavaScript Map.
    // The browser repeats validation before touching persistent state.
    // https://w3c.github.io/IndexedDB/#key-construct
    const idbHost = (...args) => __hostCall(...args);
    const idbTrusted = globalThis.__markTrustedEvent;
    const pendingDatabaseRequests = new Map();
    let nextDatabaseTransactionId = 1;
    const idbInvalidKey = () => { throw new DOMException('Invalid IndexedDB key', 'DataError'); };
    const idbKey = (value, depth = 0) => {
        if (depth > 32) return idbInvalidKey();
        if (typeof value === 'number' && Number.isFinite(value))
            return { type: 'Number', value: Object.is(value, -0) ? 0 : value };
        if (typeof value === 'string') return { type: 'String', value };
        if (value instanceof Date && Number.isFinite(value.getTime()))
            return { type: 'Date', value: value.getTime() };
        if (value instanceof ArrayBuffer)
            return { type: 'Binary', value: Array.from(new Uint8Array(value)) };
        if (ArrayBuffer.isView(value))
            return { type: 'Binary', value: Array.from(
                new Uint8Array(value.buffer, value.byteOffset, value.byteLength)) };
        if (Array.isArray(value)) return {
            type: 'Array', value: value.map(item => idbKey(item, depth + 1))
        };
        return idbInvalidKey();
    };
    const idbKeyValue = key => {
        switch (key.type) {
            case 'Number': case 'String': return key.value;
            case 'Date': return new Date(key.value);
            case 'Binary': return new Uint8Array(key.value).buffer;
            case 'Array': return key.value.map(idbKeyValue);
            default: return idbInvalidKey();
        }
    };
    const idbCompare = (left, right) => {
        const rank = { Number: 0, Date: 1, String: 2, Binary: 3, Array: 4 };
        if (rank[left.type] !== rank[right.type]) return rank[left.type] - rank[right.type];
        if (left.type === 'Number' || left.type === 'Date')
            return Math.sign(left.value - right.value);
        if (left.type === 'String')
            return left.value < right.value ? -1 : left.value > right.value ? 1 : 0;
        const a = left.value, b = right.value;
        for (let index = 0; index < Math.min(a.length, b.length); index++) {
            const order = left.type === 'Array'
                ? idbCompare(a[index], b[index]) : a[index] - b[index];
            if (order) return Math.sign(order);
        }
        return Math.sign(a.length - b.length);
    };
    const idbSend = (request, command, callback) => {
        const id = Number(idbHost('databaseRequest', JSON.stringify(command)));
        pendingDatabaseRequests.set(id, { request, callback });
        return id;
    };
    const idbFire = (target, type, event = new Event(type), abortOnException = false) => {
        event = idbTrusted(event);
        target.dispatchEvent(event);
        const handler = target['on' + type];
        if (typeof handler === 'function') {
            try { handler.call(target, event); }
            catch (error) {
                if (abortOnException) throw error;
                queueMicrotask(() => { throw error; });
            }
        }
        return event;
    };
    const idbError = (response, fallback = 'UnknownError') =>
        new DOMException(String(response.message || 'IndexedDB operation failed'),
            String(response.name || fallback));
    class IDBVersionChangeEvent extends Event {
        constructor(type, init = {}) {
            super(type, init);
            this.oldVersion = Number(init.oldVersion) || 0;
            this.newVersion = init.newVersion == null ? null : Number(init.newVersion);
        }
    }
    class IDBRequest extends EventTarget {
        constructor(source = null, transaction = null) {
            super();
            this.source = source;
            this.transaction = transaction;
            this.onsuccess = null;
            this.onerror = null;
            this._ready = false;
            this._upgrade = false;
            this._result = undefined;
            this._error = null;
        }
        get readyState() { return this._ready ? 'done' : 'pending'; }
        get result() {
            if (!this._ready && !this._upgrade)
                throw new DOMException('Request is pending', 'InvalidStateError');
            return this._result;
        }
        get error() {
            if (!this._ready && !this._upgrade)
                throw new DOMException('Request is pending', 'InvalidStateError');
            return this._error;
        }
        _succeed(result) {
            this._result = result;
            this._error = null;
            this._ready = true;
            this._upgrade = false;
            idbFire(this, 'success');
        }
        _fail(error) {
            this._error = error;
            this._ready = true;
            this._upgrade = false;
            idbFire(this, 'error', new Event('error', { bubbles: true, cancelable: true }));
        }
    }
    class IDBOpenDBRequest extends IDBRequest {
        constructor() {
            super();
            this.onupgradeneeded = null;
            this.onblocked = null;
        }
    }
    class DOMStringList {
        constructor(values) {
            const sorted = [...values].sort();
            this.length = sorted.length;
            sorted.forEach((value, index) => Object.defineProperty(this, index, {
                value, enumerable: true
            }));
        }
        contains(value) { return this.item([...this].indexOf(String(value))) !== null; }
        item(index) { return this[Number(index)] ?? null; }
        [Symbol.iterator]() {
            return Array.from({ length: this.length }, (_, index) => this[index])[Symbol.iterator]();
        }
    }
    globalThis.IDBVersionChangeEvent = IDBVersionChangeEvent;
    globalThis.IDBRequest = IDBRequest;
    globalThis.IDBOpenDBRequest = IDBOpenDBRequest;
    globalThis.DOMStringList = DOMStringList;
