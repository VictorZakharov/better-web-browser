(() => {
    'use strict';
    const host = (...args) => __hostCall(...args);
    const pending = new Map();

    globalThis.fetch = function fetch(input, init = undefined) {
        let request;
        try {
            request = new Request(input, init);
            request.signal.throwIfAborted();
        } catch (error) {
            return Promise.reject(error);
        }

        return new Promise((resolve, reject) => {
            const operation = { id: null, request, resolve, reject, settled: false };
            operation.abort = () => {
                if (operation.settled) return;
                operation.settled = true;
                if (operation.id !== null) {
                    pending.delete(operation.id);
                    host('fetchAbort', operation.id);
                }
                reject(request.signal.reason);
            };
            request.signal.addEventListener('abort', operation.abort, { once: true });
            request.__serialize().then(serialized => {
                if (operation.settled) return;
                try {
                    operation.id = Number(host('fetchStart', JSON.stringify(serialized)));
                    pending.set(operation.id, operation);
                } catch (error) {
                    operation.settled = true;
                    request.signal.removeEventListener('abort', operation.abort);
                    reject(error);
                }
            }, error => {
                if (operation.settled) return;
                operation.settled = true;
                request.signal.removeEventListener('abort', operation.abort);
                reject(error);
            });
        });
    };

    globalThis.__completeFetch = (id, serialized, body) => {
        const operation = pending.get(Number(id));
        if (!operation || operation.settled) return;
        operation.settled = true;
        pending.delete(Number(id));
        operation.request.signal.removeEventListener('abort', operation.abort);
        const metadata = JSON.parse(String(serialized));
        if (metadata.errorName) {
            operation.reject(metadata.errorName === 'AbortError'
                ? new DOMException(metadata.errorMessage, 'AbortError')
                : new TypeError(metadata.errorMessage));
            return;
        }
        const nullBody = operation.request.method === 'HEAD' ||
            [101, 204, 205, 304].includes(metadata.status) ||
            ['opaque', 'opaqueredirect', 'error'].includes(metadata.responseType);
        operation.resolve(Response.__fromNetwork(metadata, body, nullBody));
    };
})();
