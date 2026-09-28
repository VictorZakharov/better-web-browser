// Storage Standard §8. The default bucket is best-effort; there is no
// permission or persistent-bucket transition yet, so persist() is not exposed.
(() => {
    'use strict';
    const host = (...args) => __hostCall(...args);
    if (!host('storageManagerSecureContext')) return;

    const brand = new WeakSet();
    const pending = new Map();
    const token = {};
    const assertReceiver = value => {
        if (!brand.has(value)) throw new TypeError('Illegal StorageManager receiver');
    };

    class StorageManager {
        constructor(secret) {
            if (secret !== token) throw new TypeError('Illegal constructor');
            brand.add(this);
        }
        persisted() {
            assertReceiver(this);
            // No bucket in this browser can be promoted to persistent mode.
            return Promise.resolve(false);
        }
        estimate() {
            assertReceiver(this);
            return new Promise((resolve, reject) => {
                try {
                    const id = Number(host('databaseRequest', JSON.stringify({kind:'storageEstimate'})));
                    pending.set(id, {resolve, reject});
                } catch (error) { reject(error); }
            });
        }
    }
    Object.defineProperty(StorageManager.prototype, Symbol.toStringTag,
        {configurable: true, value: 'StorageManager'});
    const storage = new StorageManager(token);
    Object.defineProperty(globalThis.navigator, 'storage', {
        configurable: true, enumerable: true, get: () => storage
    });
    globalThis.StorageManager = StorageManager;

    globalThis.__receiveStorageManagerEvent = (id, payload) => {
        const operation = pending.get(Number(id));
        if (!operation) return;
        pending.delete(Number(id));
        let reply;
        try { reply = JSON.parse(String(payload)); }
        catch (_) {
            operation.reject(new DOMException('Malformed storage estimate', 'UnknownError'));
            return;
        }
        if (reply.kind === 'error') {
            const message = String(reply.message || 'Storage estimate is unavailable');
            operation.reject(reply.name === 'TypeError' ? new TypeError(message)
                : new DOMException(message, String(reply.name || 'UnknownError')));
        } else if (reply.kind === 'storageEstimate'
            && Number.isSafeInteger(reply.value?.usage) && reply.value.usage >= 0
            && Number.isSafeInteger(reply.value?.quota) && reply.value.quota >= 0) {
            operation.resolve({usage: reply.value.usage, quota: reply.value.quota});
        } else {
            operation.reject(new DOMException('Malformed storage estimate', 'UnknownError'));
        }
    };
})();
