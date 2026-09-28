// Clipboard API: browser-owned, OS-backed plain text only.
// https://w3c.github.io/clipboard-apis/#async-clipboard-api
(() => {
    'use strict';
    const native = globalThis.__hostCall;
    if (!native('clipboardAvailable')) {
        // The embedder captures this hook in every realm, even where the
        // public Clipboard API is unavailable.
        globalThis.__receiveClipboardUpdate = () => {};
        return;
    }
    const pending = new Map();
    const internal = Symbol('Clipboard');

    class Clipboard {
        constructor(key) {
            if (key !== internal) throw new TypeError('Illegal constructor');
        }
        readText() {
            if (this !== clipboard) return Promise.reject(new TypeError('Illegal Clipboard invocation'));
            return enqueue('readText');
        }
        writeText(data) {
            if (this !== clipboard) return Promise.reject(new TypeError('Illegal Clipboard invocation'));
            if (arguments.length < 1) return Promise.reject(new TypeError('Clipboard.writeText requires data'));
            try {
                // Web IDL DOMString uses ToString; String(Symbol) is a special
                // ECMAScript convenience conversion and must not be accepted.
                if (typeof data === 'symbol') throw new TypeError('Cannot convert a Symbol to a string');
                return enqueue('writeText', String(data));
            } catch (error) { return Promise.reject(error); }
        }
    }
    const enqueue = (kind, data) => new Promise((resolve, reject) => {
        try {
            const id = Number(native('clipboardRequest', kind, data ?? ''));
            pending.set(id, { resolve, reject });
        } catch (error) { reject(error); }
    });
    const clipboard = new Clipboard(internal);
    Object.defineProperty(navigator, 'clipboard', {
        configurable: true, enumerable: true, get: () => clipboard
    });
    globalThis.Clipboard = Clipboard;
    globalThis.__receiveClipboardUpdate = payload => {
        const update = JSON.parse(String(payload));
        const id = Number(update.id);
        const item = pending.get(id);
        if (!item) return;
        pending.delete(id);
        if (update.error) item.reject(new DOMException(update.message, update.error));
        else item.resolve(update.text);
    };
})();
