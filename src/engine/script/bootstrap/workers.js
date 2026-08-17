(() => {
    'use strict';
    const host = (...args) => __hostCall(...args);
    const markTrusted = globalThis.__markTrustedEvent;
    const workers = new Map();
    const defineHandler = (prototype, type) => Object.defineProperty(prototype, 'on' + type, {
        configurable: true, enumerable: true,
        get() { return this['__on' + type] || null; },
        set(value) {
            const listener = '__listener' + type;
            if (this[listener]) this.removeEventListener(type, this[listener]);
            this['__on' + type] = typeof value === 'function' ? value : null;
            this[listener] = this['__on' + type] ? event => this['__on' + type]?.call(this, event) : null;
            if (this[listener]) this.addEventListener(type, this[listener]);
        }
    });
    const missingWorkerUrl = {};
    class Worker extends EventTarget {
        constructor(url = missingWorkerUrl, options = {}) {
            super();
            if (url === missingWorkerUrl) throw new TypeError('Worker requires a script URL');
            const resolvedUrl = new URL(String(url)).href;
            if (new URL(resolvedUrl).origin !== location.origin)
                throw new DOMException('Worker scripts must be same-origin', 'SecurityError');
            const normalized = {
                type: options?.type === undefined ? 'classic' : String(options.type),
                name: options?.name === undefined ? '' : String(options.name),
                credentials: options?.credentials === undefined ? 'same-origin' : String(options.credentials)
            };
            this.__id = Number(host('workerStart', resolvedUrl, JSON.stringify(normalized)));
            this.__terminated = false;
            workers.set(this.__id, this);
        }
        postMessage(message, transfer = undefined) {
            if (this.__terminated) return;
            const transfers = __cloneTransferList(transfer);
            const serialized = __serializeClone(message, transfers);
            host('workerPostMessage', this.__id, serialized);
        }
        terminate() {
            if (this.__terminated) return;
            this.__terminated = true;
            workers.delete(this.__id);
            host('workerTerminate', this.__id);
        }
    }
    defineHandler(Worker.prototype, 'message');
    defineHandler(Worker.prototype, 'messageerror');
    defineHandler(Worker.prototype, 'error');
    globalThis.Worker = Worker;
    globalThis.__completeWorkerEvent = (id, kind, payload) => {
        const worker = workers.get(Number(id));
        if (!worker || worker.__terminated) return;
        if (kind === 'message') {
            try { worker.dispatchEvent(markTrusted(new MessageEvent('message', { data: __deserializeClone(String(payload)) }))); }
            catch (_) { worker.dispatchEvent(markTrusted(new MessageEvent('messageerror'))); }
        } else {
            worker.dispatchEvent(markTrusted(new ErrorEvent('error', { message: String(payload) })));
        }
    };
    delete globalThis.__markTrustedEvent;
})();
