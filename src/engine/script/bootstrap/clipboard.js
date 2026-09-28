// Clipboard API: browser-owned, OS-backed plain text only.
// https://w3c.github.io/clipboard-apis/#async-clipboard-api
(() => {
    'use strict';
    const native = globalThis.__hostCall;
    if (!native('clipboardAvailable')) return;
    const pending = new Map();
    const internal = Symbol('Clipboard');

    class Clipboard {
        constructor(key) {
            if (key !== internal) throw new TypeError('Illegal constructor');
        }
        readText() {
            if (this !== clipboard) throw new TypeError('Illegal Clipboard invocation');
            return enqueue('readText');
        }
        writeText(data) {
            if (this !== clipboard) throw new TypeError('Illegal Clipboard invocation');
            if (arguments.length < 1) throw new TypeError('Clipboard.writeText requires data');
            return enqueue('writeText', String(data));
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
